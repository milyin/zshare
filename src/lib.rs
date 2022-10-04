#[macro_use]
extern crate lazy_static;

mod error;

pub use error::Error;
pub use error::Result;

use futures::task::{Spawn, SpawnError, SpawnExt};
use serde::Serialize;
use std::sync::Arc;
use std::sync::RwLock;
use uuid::Uuid;
use zenoh::prelude::sync::SyncResolve;
use zenoh::queryable::Query;
use zenoh::sample::Sample;
use zenoh::value::Value;
use zenoh::{key_expr, prelude::KeyExpr, Session};

lazy_static! {
    pub static ref INSTANCE_UUID: String = Uuid::new_v4().as_urn().to_string();
}

pub struct ZSharedValue<T: Into<Value> + Send + Sync + 'static> {
    value: Arc<RwLock<T>>,
    name: String,
}

impl<T: Into<Value> + Send + Sync + 'static> ZSharedValue<T> {
    pub fn new(zsession: Session, value: T, workspace: KeyExpr, name: String) -> Result<Self> {
        let value = Arc::new(RwLock::new(value));
        let key_expr = workspace.join(&name)?.join(INSTANCE_UUID.as_str())?;
        let callback = {
            let value = value.clone();
            |query: Query| {
                let value = value.read().unwrap();
                let value: Value = (*value.read().unwrap()).into();
                let sample = Sample::new(query.key_expr(), value);
                query.reply(Ok(sample)).res_sync();
            }
        };
        let queryable = zsession
            .declare_queryable(&key_expr)
            .callback(callback)
            .res_sync()?;
        Ok(Self { value, name })
    }
    pub async fn update(&self, value: T) {
        *self.value.write().await = value; // TODO: use update function
                                           // TODO: broadcast update
    }
}

pub struct ZSharedReader<T: Serialize> {
    value: T,
    key: String,
}

impl<T: Serialize> ZSharedReader<T> {}
