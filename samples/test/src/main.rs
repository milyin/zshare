use async_std::io::ReadExt;
use serde::{Deserialize, Serialize};
use zenoh::prelude::{r#async::AsyncResolve, Config, KeyExpr};
use zshare::{Update, ZSharedValue, ZSharedView};

#[derive(Default, Serialize, Deserialize)]
struct Value(u64);
#[derive(Serialize, Deserialize)]
enum Change {
    Inc,
    Dec,
}

impl Update for Value {
    type Command = Change;

    fn update(&mut self, command: Self::Command) {
        match command {
            Change::Inc => self.0 += 1,
            Change::Dec => self.0 -= 1,
        }
    }
}

#[async_std::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    println!("Opening session...");
    let session = zenoh::open(Config::default()).res().await.unwrap();

    let workspace = KeyExpr::new("workspace").unwrap();
    let name = KeyExpr::new("name").unwrap();
    let data = ZSharedValue::new(&session, Value(42), &workspace, name.clone()).unwrap();
    let view = ZSharedView::<Value, Change>::new(&session, &workspace, name.clone()).unwrap();

    let mut stdin = async_std::io::stdin();
    let mut input = [0_u8];
    loop {
        stdin.read_exact(&mut input).await.unwrap();
        match input[0] {
            b'q' => break,
            b'i' => data.update(Change::Inc),
            b'd' => data.update(Change::Dec),
            b'p' => println!("{} {}\n", data.read().0, view.read().0),
            _ => ()
            // 0 => sleep(Duration::from_secs(1)).await,
            // _ => subscriber.pull().res().await.unwrap(),
        }
    }
}
