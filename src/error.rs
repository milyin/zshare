use futures::task::SpawnError;
use thiserror::Error

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Spawn(SpawnError),
    #[error(transparent)]
    Zenoh(zenoh::Error)
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<SpawnError> for Error {
    fn from(e: SpawnError) -> Self {
        Error::Spawn(e)
    }
}

impl From<zenoh::Error> for Error {
    fn from(e: zenoh::Error) -> Self {
        Error::Zenoh(e)
    }
}