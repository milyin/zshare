use async_std::io::ReadExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use zenoh::{
    prelude::{r#async::AsyncResolve, Config, KeyExpr},
    Session,
};
use zshare::{
    get_data_path, get_instance_path, get_update_path, Result, Update, ZSharedInstances,
    ZSharedValue, ZSharedView, INSTANCE,
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

fn refresh_views_sync<'a, 'b>(
    session: &'a Session,
    workspace: &KeyExpr,
    name: &KeyExpr<'static>,
    views: &'b mut Vec<ZSharedView<'a, Value, Change>>,
) -> Result<()> {
    let mut instances: HashSet<_> = ZSharedInstances::new(workspace, name)?
        .iter(session)?
        .collect();
    views.retain(|v| instances.remove(&v.instance().clone().into_owned()));
    for instance in instances {
        views.push(ZSharedView::new(
            session,
            workspace,
            instance,
            name.clone(),
        )?)
    }
    Ok(())
}

fn print(value: &ZSharedValue<Value, Change>, views: &Vec<ZSharedView<Value, Change>>) {
    println!("value {} {}", &INSTANCE.as_str(), value.read().0);
    for v in views {
        println!("view {} {}", v.instance(), v.read().0)
    }
}

#[async_std::main]
async fn main() -> zshare::Result<()> {
    // Initiate logging
    env_logger::init();

    println!("Opening session...");
    let session = zenoh::open(Config::default()).res().await?;
    let mut views = Vec::new();
    let workspace = KeyExpr::new("workspace")?;
    let name = KeyExpr::new("name")?;
    let data = ZSharedValue::new(&session, Value(42), &workspace, name.clone())?;

    println!(
        "Commands: (p)rint, (i)nc, (d)ec, (r)efresh, (q)uit\nKeys:\n{}\n{}\n{}",
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
            b'p' => print(&data, &views),
            b'r' => refresh_views_sync(&session, &workspace, &name, &mut views)?,
            _ => (),
        }
    }
    Ok(())
}
