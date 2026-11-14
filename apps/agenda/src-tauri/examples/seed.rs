//! Sample data for Agenda v2 (testing, screenshots). Times are relative to now.
//! `SELFKIT_DATA_DIR=/tmp/x cargo run -p agenda --example seed`
//! `--theme light|dark` only sets the app theme preference.

use agenda_lib::tasks::{self, NewTask};
use chrono::{Duration, Local, Timelike};
use selfkit_core::{Store, paths};
use serde_json::json;

fn main() -> selfkit_core::Result<()> {
    let device = paths::device_id()?;
    let s = Store::open(paths::app_db("agenda"), "agenda", &device, tasks::MIGRATIONS)?;
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--theme") {
        s.kv_set("agenda.prefs", &json!({"theme": args[i + 1]}).to_string())?;
        return Ok(());
    }
    let now = Local::now().naive_local().with_second(0).unwrap().with_nanosecond(0).unwrap();
    let at = |days: i64, h: u32, m: u32| (now.date() + Duration::days(days)).and_hms_opt(h, m, 0).unwrap().format("%Y-%m-%dT%H:%M").to_string();
    let rel = |mins: i64| (now + Duration::minutes(mins)).format("%Y-%m-%dT%H:%M").to_string();
    let study = tasks::add_list(&s, "Study")?;
    let add = |t: NewTask| tasks::create(&s, t).unwrap();
    add(NewTask {
        title: "NASA Space Challenge video script".into(),
        notes: Some("Draft the 2-minute script and storyboard. Keep it under 300 words.".into()),
        severity: Some(5), start: Some(at(-2, 12, 0)), stop: Some(at(-2, 14, 0)), reminder_min: Some(30), list: Some(study.clone()),
        steps: Some(vec![json!({"id":"a","title":"Collect facts and images","done":true}), json!({"id":"b","title":"Write the script","done":false}), json!({"id":"c","title":"Storyboard 6 scenes","done":false})]),
        ..Default::default()
    });
    add(NewTask { title: "Call the landlord about the leak".into(), notes: Some("Water under the sink again.".into()), severity: Some(3), start: Some(rel(25)), stop: Some(rel(55)), reminder_min: Some(10), ..Default::default() });
    add(NewTask { title: "Study chapter 6, linear algebra".into(), severity: Some(2), start: Some(at(0, 22, 0)), stop: Some(at(0, 23, 30)), repeat: Some("weekly".into()), list: Some(study), ..Default::default() });
    add(NewTask { title: "Pay rent".into(), severity: Some(4), start: Some(at(1, 17, 0)), reminder_min: Some(60), ..Default::default() });
    add(NewTask { title: "Dentist".into(), severity: Some(3), start: Some(at(4, 15, 30)), stop: Some(at(4, 16, 15)), reminder_min: Some(120), ..Default::default() });
    add(NewTask { title: "Read Dune".into(), severity: Some(1), ..Default::default() });
    let d1 = add(NewTask { title: "Reply to Sam".into(), severity: Some(2), ..Default::default() });
    tasks::toggle(&s, &d1.id, now.date())?;
    let d2 = add(NewTask { title: "Morning run".into(), severity: Some(3), start: Some(at(0, 6, 30)), ..Default::default() });
    tasks::toggle(&s, &d2.id, now.date())?;
    println!("seeded {}", paths::data_dir().display());
    Ok(())
}
