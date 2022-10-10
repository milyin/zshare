use async_std::io::ReadExt;
use serde::{Deserialize, Serialize};
use zenoh::prelude::{r#async::AsyncResolve, Config, KeyExpr};
use zshare::{
    get_data_path, get_update_path, query_instances, query_instances_async, Update, ZSharedValue,
    ZSharedView, INSTANCE, get_instance_path,
};

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
async fn main() -> zshare::Result<()> {
    // Initiate logging
    env_logger::init();

    println!("Opening session...");
    let session = zenoh::open(Config::default()).res().await?;

    let workspace = KeyExpr::new("workspace")?;
    let name = KeyExpr::new("name")?;
    let data = ZSharedValue::new(&session, Value(42), &workspace, name.clone())?;
    let view =
        ZSharedView::<Value, Change>::new(&session, &workspace, INSTANCE.clone(), name.clone())?;

    println!(
        "Commands: p, i, d, q\n{}\n{}\n{}",
        get_instance_path(&workspace, &INSTANCE, &name)?,
        get_data_path(&workspace, &INSTANCE, &name)?,
        get_update_path(&workspace, &INSTANCE, &name)?
    );

    let mut stdin = async_std::io::stdin();
    let mut input = [0_u8];
    loop {
        stdin.read_exact(&mut input).await.unwrap();
        match input[0] {
            b'q' => break,
            b'i' => data.update(Change::Inc),
            b'd' => data.update(Change::Dec),
            b'p' => println!("{} {}", data.read().0, view.read().0),
            b's' => {
                let instances = query_instances_async(&session, &workspace, &name).await?;
                if !instances.is_empty() {
                    println!("Instances:");
                    for v in &instances {
                        println!("{}", v);
                    }
                }
            },
            _ => ()
            // 0 => sleep(Duration::from_secs(1)).await,
            // _ => subscriber.pull().res().await.unwrap(),
        }
    }
    Ok(())
}
