//! Print the nights, stats and connections Dusk computes for a data folder
//! (debugging the estimate without the UI).
//! `SELFKIT_DATA_DIR=/tmp/dusk-demo cargo run -p dusk --example dusk_report [nights]`

use chrono::Duration;
use dusk_lib::{data, model, sources};
use selfkit_core::{Store, paths};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let n: i64 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(14);
    let store = Store::open(paths::app_db("dusk"), "dusk", &paths::device_id()?, data::MIGRATIONS)?;
    let tz = selfkit_core::time::local_offset_min();
    let settings = Store::open(paths::app_db("settings"), "settings", &paths::device_id()?, &[])?;
    let st = selfkit_core::settings::Settings::load(&settings)?;
    let ds = st.day_start_min;
    let cur = model::current_night(selfkit_core::time::now_ms(), tz, ds);
    let ns = dusk_lib::compute(&store, cur - Duration::days(89), cur, false)?;
    let hm = |t: Option<i64>, tz: i32| t.map(|t| model::local_dt(t, tz).format("%H:%M").to_string()).unwrap_or("--:--".into());
    for v in &ns[ns.len() - n as usize..] {
        println!(
            "{} {:>10} est {}–{} ({:>3}m awake {}x) eff {}–{} asleep {:>4} nap {:>3} q {:?} late {:>3} tz {}",
            v.night, v.source, hm(v.estimate.bed, v.tz_offset), hm(v.estimate.wake, v.tz_offset), v.estimate.awake_min,
            v.estimate.interruptions, hm(v.bed, v.tz_offset), hm(v.wake, v.tz_offset), v.asleep_min, v.nap_min, v.quality, v.late_use_min, v.tz_offset
        );
    }
    let g = data::goals(&store)?.model();
    println!("{:?}", model::stats(&ns, 7, &g));
    println!("{:?}", model::stats(&ns, 14, &g));
    let today = model::logical_day(selfkit_core::time::now_ms(), tz, 0);
    println!("{:?}", model::late_use_vs_quality(&ns, "Chrono"));
    let a = sources::open_other("agenda").map(|a| sources::tasks_done_per_day(&a, 0, today)).unwrap_or_default();
    println!("{:?}", model::sleep_vs_next_day(&ns, &a, 0, "tasks", "tasks", true));
    let h = sources::open_other("hisab").map(|h| sources::spent_per_day(&h, 0, today, &st.main_currency)).unwrap_or_default();
    println!("{:?}", model::sleep_vs_next_day(&ns, &h, 0, "money", "$", true));
    Ok(())
}
