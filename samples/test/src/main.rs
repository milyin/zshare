use zenoh::prelude::{r#async::AsyncResolve, Config};

#[async_std::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    println!("Opening session...");
    let session = zenoh::open(Config::default()).res().await.unwrap();
}
