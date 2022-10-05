#[macro_use]
extern crate lazy_static;

mod error;

pub use error::Error;
pub use error::Result;

use futures::task::{Spawn, SpawnError, SpawnExt};
use rmp_serde::Serializer;
use serde::Serialize;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use uuid::Uuid;
use zenoh::handlers::Callback;
use zenoh::prelude::sync::SyncResolve;
use zenoh::publication::Publisher;
use zenoh::queryable::Query;
use zenoh::queryable::Queryable;
use zenoh::sample::Sample;
use zenoh::value::Value;
use zenoh::{key_expr, prelude::KeyExpr, Session};

lazy_static! {
    pub static ref INSTANCE_ID: String = Uuid::new_v4().as_urn().to_string();
}

pub trait Update {
    type Command;
    fn update(&mut self, command: Self::Command);
}

fn get_paths<'a>(workspace: &'a KeyExpr, name: &'a KeyExpr) -> (KeyExpr<'a>, KeyExpr<'a>) {
    let path = workspace
        .join(&name)?
        .join(INSTANCE_ID.as_str())?
        .join("data")?;
    let query_path = path.join("data")?;
    let pub_path = path.join("update")?;
    (query_path, pub_path)
}

pub struct ZSharedValue<
    'a,
    DATA: Update<Command = COMMAND> + Serialize + Send + Sync + 'static,
    COMMAND: Serialize,
> {
    data: Arc<RwLock<DATA>>,
    name: KeyExpr<'static>,
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
        workspace: KeyExpr,
        name: KeyExpr<'static>,
    ) -> Result<Self> {
        let data = Arc::new(RwLock::new(data));
        let (query_path, pub_path) = get_paths(&workspace, &name);
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
            .declare_queryable(&query_path)
            .callback(callback)
            .res_sync()?;
        let publisher = zsession.declare_publisher(pub_path).res_sync()?;
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

    pub fn update(&self, command: COMMAND) {
        let mut buf = Vec::new();
        command.serialize(&mut Serializer::new(&mut buf)).unwrap();
        self.publisher.put(buf);
        let mut data = self.data.write().unwrap();
        data.update(command);
    }
}

pub struct ZShareView<
    'a,
    DATA: Update<Command = COMMAND> + Serialize + Send + Sync + 'static,
    COMMAND: Serialize,
> {
    data: Arc<RwLock<Option<DATA>>>,
    name: KeyExpr<'static>,
}

impl<
        'a,
        DATA: Update<Command = COMMAND> + Serialize + Send + Sync + 'static,
        COMMAND: Serialize,
    > ZShareView<'a, DATA, COMMAND>
{
    pub fn new(zsession: &'a Session, workspace: KeyExpr, name: KeyExpr<'static>) -> Result<Self> {
        let data = Arc::new(RwLock::new(None));
        let (query_path, pub_path) = get_paths(&workspace, &name);
        let subscriber = zsession
            .declare_subscriber(pub_path)
            .callback(callback)
            .res_sync()?;
        Ok(Self { data, name })
    }
}

/*
pub struct ZSharedValue<
    DATA: AsRef<SNAPSHOT> + Send + Sync + 'static,
    SNAPSHOT: Clone + Into<Value>,
> {
    value: Arc<RwLock<DATA>>,
    name: String, // TODO: store as KeyExpr
    _snapshot: PhantomData<SNAPSHOT>,
}

impl<DATA: AsRef<SNAPSHOT> + Send + Sync + 'static, SNAPSHOT: Clone + Into<Value>>
    ZSharedValue<DATA, SNAPSHOT>
{
    pub fn new(zsession: Session, value: DATA, workspace: KeyExpr, name: String) -> Result<Self> {
        let value = Arc::new(RwLock::new(value));
        let key_expr = workspace.join(&name)?.join(INSTANCE_ID.as_str())?;
        let callback = {
            let value = value.clone();
            move |query: Query| {
                let value = value.read().unwrap();
                let value: SNAPSHOT = value.as_ref().clone();
                let value: Value = value.into();
                let sample = Sample::new(query.key_expr().clone(), value);
                query.reply(Ok(sample)).res_sync().unwrap();
            }
        };
        let queryable = zsession
            .declare_queryable(&key_expr)
            .callback(callback)
            .res_sync()?;
        Ok(Self {
            value,
            name,
            _snapshot: PhantomData::default(),
        })
    }
    pub fn update(&self, value: DATA) {
        *self.value.write().unwrap() = value; // TODO: use update function
                                              // TODO: broadcast update
    }
}

pub struct ZSharedReader<DATA: TryFrom<Value> + Send + Sync + 'static> {
    value: Arc<RwLock<Option<DATA>>>,
    name: String,
}

impl<DATA: From<Value> + Send + Sync + 'static> ZSharedReader<DATA> {
    pub fn new(
        zsession: Session,
        value: DATA,
        workspace: KeyExpr,
        instance_id: String,
        name: String,
    ) -> Result<Self> {
        let value = Arc::new(RwLock::new(None));
        let key_expr = workspace.join(&name)?.join(instance_id.as_str())?;
        let callback = {
            let value = value.clone();
            move |sample: Sample| {
                if let Ok(new_value) = sample.value.try_into() {
                    let mut value = value.write().unwrap();
                    *value = Some(new_value);
                }
            }
        };
        let subscriber = zsession
            .declare_subscriber(key_expr)
            .callback(callback)
            .res_sync()?;
        Ok(Self { value, name })
    }
}

#[async_std::test]
async fn simple_test() {
    println!("FOOOO");
}
*/
