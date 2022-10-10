#[macro_use]
extern crate lazy_static;

mod error;

pub use error::Error;
pub use error::Result;

use flume::Receiver;
use rmp_serde::Deserializer;
use rmp_serde::Serializer;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use std::marker::PhantomData;
use std::str::from_utf8;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use uuid::Uuid;
use zenoh::prelude::r#async::AsyncResolve;
use zenoh::prelude::sync::SyncResolve;
use zenoh::prelude::SplitBuffer;
use zenoh::publication::Publisher;
use zenoh::query::Reply;
use zenoh::queryable::Query;
use zenoh::queryable::Queryable;
use zenoh::sample::Sample;
use zenoh::subscriber::Subscriber;
use zenoh::{prelude::KeyExpr, Session};

lazy_static! {
    pub static ref INSTANCE: KeyExpr<'static> = {
        let uuid = Uuid::new_v4().as_hyphenated().to_string();
        unsafe { KeyExpr::from_string_unchecked(uuid) }
    };
    static ref STAR: KeyExpr<'static> = unsafe { KeyExpr::from_str_uncheckend("*") };
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

pub fn get_instance_path(
    workspace: &KeyExpr,
    instance: &KeyExpr,
    name: &KeyExpr,
) -> Result<KeyExpr<'static>> {
    Ok(workspace.join(&name)?.join(instance)?.join("instance")?)
}

pub struct ZSharedValue<
    'a,
    DATA: Update<Command = COMMAND> + Serialize + Send + Sync + 'static,
    COMMAND: Serialize,
> {
    data: Arc<RwLock<DATA>>,
    name: KeyExpr<'a>,
    publisher: Publisher<'a>,
    _queryable_data: Queryable<'a, ()>,
    _queryable_instance: Queryable<'a, ()>,
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
        let data_path = get_data_path(&workspace, &INSTANCE, &name)?;
        let update_path = get_update_path(&workspace, &INSTANCE, &name)?;
        let instance_path = get_instance_path(&workspace, &INSTANCE, &name)?;
        let data_callback = {
            let data = data.clone();
            let data_path = data_path.clone();
            move |query: Query| {
                let data = data.read().unwrap();
                let mut buf = Vec::new();
                data.serialize(&mut Serializer::new(&mut buf)).unwrap();
                let sample = Sample::new(data_path.clone(), buf);
                query.reply(Ok(sample)).res_sync().unwrap();
            }
        };
        let instance_callback = {
            let instance_path = instance_path.clone();
            move |query: Query| {
                let sample = Sample::new(instance_path.clone(), INSTANCE.as_str().as_bytes());
                query.reply(Ok(sample)).res_sync().unwrap();
            }
        };
        let _queryable_data = zsession
            .declare_queryable(&data_path)
            .callback(data_callback)
            .res_sync()?;
        let _queryable_instance = zsession
            .declare_queryable(&instance_path)
            .callback(instance_callback)
            .res_sync()?;
        let publisher = zsession.declare_publisher(update_path).res_sync()?;
        Ok(Self {
            data,
            name,
            publisher,
            _queryable_data,
            _queryable_instance,
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
        self.publisher.put(buf).res_sync().unwrap();
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
        &self.instance
    }
}

// pub fn query_instances(
//     zsession: &Session,
//     workspace: &KeyExpr,
//     name: &KeyExpr,
// ) -> Result<Vec<KeyExpr<'static>>> {
//     let path = get_instance_path(workspace, &STAR, name)?;
//     let query = zsession.get(path).res_sync()?;
//     let mut res = Vec::new();
//     while let Ok(reply) = query.recv() {
//         if let Ok(sample) = reply.sample {
//             let buf = sample.payload.contiguous();
//             if let Ok(s) = from_utf8(&buf) {
//                 if let Ok(keyexpr) = KeyExpr::from_str(s) {
//                     res.push(keyexpr);
//                 }
//             }
//         }
//     }
//     Ok(res)
// }

pub struct ZSharedInstances {
    path: KeyExpr<'static>,
}

impl ZSharedInstances {
    pub fn new(workspace: &KeyExpr, name: &KeyExpr) -> Result<Self> {
        let path = get_instance_path(workspace, &STAR, name)?;
        Ok(Self { path })
    }
    pub fn iter<'a>(&'a self, session: &'a Session) -> Result<ZSharedInstancesIter<'a>> {
        ZSharedInstancesIter::new(session, &self.path)
    }
}

pub struct ZSharedInstancesIter<'a> {
    _session: &'a Session,
    query: Receiver<Reply>,
}

impl<'a> ZSharedInstancesIter<'a> {
    fn new(session: &'a Session, path: &'a KeyExpr<'a>) -> Result<Self> {
        let query = session.get(path).res_sync()?;
        Ok(Self {
            _session: session,
            query,
        })
    }
}

impl<'a> Iterator for ZSharedInstancesIter<'a> {
    type Item = KeyExpr<'static>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Ok(reply) = self.query.try_recv() {
            if let Ok(sample) = reply.sample {
                let buf = sample.payload.contiguous();
                if let Ok(s) = from_utf8(&buf) {
                    if let Ok(keyexpr) = KeyExpr::from_str(s) {
                        return Some(keyexpr);
                    }
                }
            }
        }
        None
    }
}

pub async fn query_instances_async<'a>(
    zsession: &'a Session,
    workspace: &'a KeyExpr<'a>,
    name: &'a KeyExpr<'a>,
) -> Result<Vec<KeyExpr<'static>>> {
    let path = get_instance_path(workspace, &STAR, name)?;
    let query = zsession.get(path).res_async().await?;
    let mut res = Vec::new();
    while let Ok(reply) = query.recv_async().await {
        if let Ok(sample) = reply.sample {
            let buf = sample.payload.contiguous();
            if let Ok(s) = from_utf8(&buf) {
                if let Ok(keyexpr) = KeyExpr::from_str(s) {
                    res.push(keyexpr);
                }
            }
        }
    }
    Ok(res)
}
