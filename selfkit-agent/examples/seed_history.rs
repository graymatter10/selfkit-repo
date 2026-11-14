//! Dev/test helper: fill chrono.db and hisab.db with records spread over the last
//! N months (default 4) so monthly backups have something to archive.
//! `SELFKIT_DATA_DIR=… cargo run -p selfkit-agent --example seed_history -- 4`
use selfkit_core::store::{Record, new_id};
use selfkit_core::{Store, paths, time};
use serde_json::json;

fn main() -> anyhow::Result<()> {
    let months: i64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let dev = paths::device_id()?;
    let now = time::now_ms();
    let day = 86_400_000;
    for app in ["chrono", "hisab"] {
        let st = Store::open(paths::app_db(app), app, &dev, &[])?;
        let mut n = 0;
        for d in 0..months * 30 {
            let at = now - d * day - 3_600_000;
            for k in 0..3 {
                let data = if app == "chrono" {
                    let a = ["firefox", "code", "alacritty"][k];
                    json!({"app": a, "platform": "linux", "start": at, "end": at + 600_000 * (k as i64 + 1), "tz_offset": 0})
                } else {
                    json!({"amount": 100 + d * 3 + k as i64, "category": "food", "note": format!("seed {d}/{k}")})
                };
                let coll = if app == "chrono" { "app_sessions" } else { "transactions" };
                let r = Record { id: new_id(), collection: coll.into(), data, updated_at: at + k as i64, tz_offset: 0, device_id: dev.clone(), deleted: false, hist: vec![] };
                st.import_record(&r)?;
                n += 1;
            }
        }
        println!("{app}: {n} records");
    }
    Ok(())
}
