//! Dev/test helper: write one record into an app DB, e.g. an Agenda task with a
//! reminder: `put_record agenda tasks t1 '{"title":"Call","start":"2026-11-20T10:30","reminder_min":10}'`.
use selfkit_core::{Store, paths};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(a.len() == 4, "usage: put_record <app> <collection> <id> <json>");
    let store = Store::open(paths::app_db(&a[0]), &a[0], &paths::device_id()?, &[])?;
    let r = store.put(&a[1], Some(&a[2]), &serde_json::from_str(&a[3])?)?;
    println!("{}", serde_json::to_string(&r)?);
    Ok(())
}
