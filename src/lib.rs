#[macro_use]
extern crate lazy_static;

mod error;

pub use error::Error;
pub use error::Result;

use futures::task::{Spawn, SpawnError, SpawnExt};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;
use zenoh::{key_expr, prelude::KeyExpr, Session};

lazy_static! {
    pub static ref INSTANCE_UUID: String = Uuid::new_v4().as_urn().to_string();
}

pub struct ZSharedValue<T: Serialize + Send + Sync + 'static> {
    value: Arc<RwLock<T>>,
    name: String,
}

impl<T: Serialize + Send + Sync + 'static> ZSharedValue<T> {
    pub fn new(
        spawner: impl Spawn,
        zsession: Session,
        value: T,
        workspace: KeyExpr,
        name: String,
    ) -> Result<Self> {
        let value = Arc::new(RwLock::new(value));
        let key_expr = workspace.join(&name)?.join(INSTANCE_UUID.as_str())?;
        let queryable = zsession.declare_queryable(&key_expr).res_sync()?;
        spawner.spawn({
            let value = Arc::downgrade(&value);
            // let key_expr =
            async move { while let Some(value) = value.upgrade() {} }
        })?;
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
