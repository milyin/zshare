use serde::Serialize;

pub struct ZSharedValue<T: Serialize> {
    value: T,
    key: String
}

impl<T: Serialize> ZSharedValue<T> {
    pub fn new(value: T, key: String) -> Self {
        Self { value, key }
    }
    pub fn update(&mut self, value: T) {
        self.value = value; // TODO: use update function
        // TODO: broadcast update
    }
}

impl<T: Serialize> AsRef<T> for ZSharedValue<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

pub struct ZSharedReader<T: Serialize> {
    value: T,
    key: String
}
