#[macro_use]
extern crate lazy_static;

mod error;

pub use error::Error;
pub use error::Result;

use rmp_serde::Deserializer;
use rmp_serde::Serializer;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use uuid::Uuid;
use zenoh::prelude::sync::SyncResolve;
use zenoh::prelude::SplitBuffer;
use zenoh::publication::Publisher;
use zenoh::queryable::Query;
use zenoh::queryable::Queryable;
use zenoh::sample::Sample;
use zenoh::subscriber::Subscriber;
use zenoh::{prelude::KeyExpr, Session};

lazy_static! {
    pub static ref INSTANCE_ID: KeyExpr<'static> = {
        let uuid = Uuid::new_v4().as_hyphenated().to_string();
        unsafe { KeyExpr::from_string_unchecked(uuid) }
    };
}

pub trait Update {
    type Command;
    fn update(&mut self, command: Self::Command);
}

pub fn get_data_path(
    workspace: &KeyExpr,
    instance: &KeyExpr,
    name: &KeyExpr,
) -> Result<KeyExpr<'static>> {
    Ok(workspace.join(name)?.join(instance)?.join("data")?)
}

pub fn get_update_path(
    workspace: &KeyExpr,
    instance: &KeyExpr,
    name: &KeyExpr,
) -> Result<KeyExpr<'static>> {
    Ok(workspace.join(&name)?.join(instance)?.join("update")?)
}

pub struct ZSharedValue<
    'a,
    DATA: Update<Command = COMMAND> + Serialize + Send + Sync + 'static,
    COMMAND: Serialize,
> {
    data: Arc<RwLock<DATA>>,
    name: KeyExpr<'a>,
    publisher: Publisher<'a>,
    _queryable: Queryable<'a, ()>,
    _command: PhantomData<COMMAND>,
}

impl<
        'a,
        DATA: Update<Command = COMMAND> + Serialize + Send + Sync + 'static,
        COMMAND: Serialize,
    > ZSharedValue<'a, DATA, COMMAND>
{
    pub fn new(
        zsession: &'a Session,
        data: DATA,
        workspace: &'a KeyExpr,
        name: KeyExpr<'a>,
    ) -> Result<Self> {
        let data = Arc::new(RwLock::new(data));
        let data_path = get_data_path(&workspace, &INSTANCE_ID, &name)?;
        let update_path = get_update_path(&workspace, &INSTANCE_ID, &name)?;
        let callback = {
            let data = data.clone();
            move |query: Query| {
                let data = data.read().unwrap();
                let mut buf = Vec::new();
                data.serialize(&mut Serializer::new(&mut buf)).unwrap();
                let sample = Sample::new(query.key_expr().clone(), buf);
                query.reply(Ok(sample)).res_sync().unwrap();
            }
        };
        let _queryable = zsession
            .declare_queryable(&data_path)
            .callback(callback)
            .res_sync()?;
        let publisher = zsession.declare_publisher(update_path).res_sync()?;
        Ok(Self {
            data,
            name,
            publisher,
            _queryable,
            _command: PhantomData::default(),
        })
    }

    pub fn read(&self) -> RwLockReadGuard<DATA> {
        self.data.read().unwrap()
    }

    pub fn name(&self) -> &KeyExpr {
        &self.name
    }

    pub fn update(&self, command: COMMAND) {
        let mut buf = Vec::new();
        command.serialize(&mut Serializer::new(&mut buf)).unwrap();
        self.publisher.put(buf).res().unwrap();
        let mut data = self.data.write().unwrap();
        data.update(command);
    }
}

pub struct ZSharedView<
    'a,
    DATA: Default + Update<Command = COMMAND> + DeserializeOwned + Send + Sync + 'static,
    COMMAND: DeserializeOwned,
> {
    data: Arc<RwLock<DATA>>,
    instance: KeyExpr<'a>,
    name: KeyExpr<'a>,
    _subscriber: Subscriber<'a, ()>,
}

impl<
        'a,
        DATA: Default + Update<Command = COMMAND> + DeserializeOwned + Send + Sync + 'static,
        COMMAND: DeserializeOwned,
    > ZSharedView<'a, DATA, COMMAND>
{
    pub fn new(
        zsession: &'a Session,
        workspace: &KeyExpr,
        instance: KeyExpr<'static>,
        name: KeyExpr<'static>,
    ) -> Result<Self> {
        let data = Arc::new(RwLock::new(DATA::default()));
        let data_path = get_data_path(&workspace, &instance, &name)?;
        let update_path = get_update_path(&workspace, &instance, &name)?;
        let update_callback = {
            let data = data.clone();
            move |sample: Sample| {
                let buf: Vec<u8> = sample.payload.contiguous().into();
                let mut deserializer = Deserializer::new(buf.as_slice());
                let mut data = data.write().unwrap();
                let command: COMMAND = Deserialize::deserialize(&mut deserializer).unwrap();
                data.update(command);
            }
        };
        let _subscriber = zsession
            .declare_subscriber(update_path)
            .callback(update_callback)
            .res_sync()?;
        let query = zsession.get(data_path).res_sync()?;
        while let Ok(reply) = query.recv() {
            if let Ok(sample) = reply.sample {
                let buf: Vec<u8> = sample.payload.contiguous().into();
                let mut deserializer = Deserializer::new(buf.as_slice());
                let mut data = data.write().unwrap();
                *data = Deserialize::deserialize(&mut deserializer).unwrap();
                break;
            }
        }
        Ok(Self {
            data,
            instance,
            name,
            _subscriber,
        })
    }

    pub fn read(&self) -> RwLockReadGuard<DATA> {
        self.data.read().unwrap()
    }

    pub fn name(&self) -> &KeyExpr {
        &self.name
    }
    pub fn instance(&self) -> &KeyExpr {
        &&self.instance
    }
}
