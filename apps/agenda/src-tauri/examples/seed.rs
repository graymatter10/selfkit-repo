//! Sample data for Agenda (testing, screenshots). Times are relative to now.
//! `SELFKIT_DATA_DIR=/tmp/x cargo run -p agenda --example seed [-- --photo a.jpg --photo b.jpg]`
//! `--theme light|dark` only sets the app theme preference.
//! Adds: open tasks with tags, Markdown notes, steps, repeats, a Doing column,
//! photos (when `--photo` files are given), ~4 months of completed-task history
//! and focus sessions (for History and Stats).

use agenda_lib::{
    focus, photos,
    tasks::{self, NewTask},
};
use chrono::{Duration, Local, TimeZone, Timelike};
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
    let photo_files: Vec<&String> = args.iter().enumerate().filter(|(i, _)| *i > 0 && args[i - 1] == "--photo").map(|(_, a)| a).collect();
    let mut photo_ids = Vec::new();
    for f in photo_files {
        let bytes = std::fs::read(f)?;
        let mime = if f.ends_with(".png") { "image/png" } else if f.ends_with(".webp") { "image/webp" } else { "image/jpeg" };
        photo_ids.push(photos::save(&s, &bytes, mime, 2400, 1800)?);
    }

    let now = Local::now().naive_local().with_second(0).unwrap().with_nanosecond(0).unwrap();
    let at = |days: i64, h: u32, m: u32| (now.date() + Duration::days(days)).and_hms_opt(h, m, 0).unwrap().format("%Y-%m-%dT%H:%M").to_string();
    let day = |days: i64| (now.date() + Duration::days(days)).format("%Y-%m-%d").to_string();
    let rel = |mins: i64| (now + Duration::minutes(mins)).format("%Y-%m-%dT%H:%M").to_string();
    let study = tasks::add_list(&s, "Study")?;
    let tags = |v: &[&str]| Some(v.iter().map(|t| t.to_string()).collect::<Vec<_>>());
    let add = |t: NewTask| tasks::create(&s, t).unwrap();
    add(NewTask {
        title: "NASA Space Challenge video script".into(),
        notes: Some("Draft the **2-minute** script and storyboard.\n\n- Keep it under 300 words\n- One idea per scene\n\nBrief: https://www.spaceappschallenge.org".into()),
        severity: Some(5), start: Some(at(-2, 12, 0)), stop: Some(at(-2, 14, 0)), reminder_min: Some(30), list: Some(study.clone()),
        steps: Some(vec![json!({"id":"a","title":"Collect facts and images","done":true}), json!({"id":"b","title":"Write the script","done":false}), json!({"id":"c","title":"Storyboard 6 scenes","done":false})]),
        tags: tags(&["school", "video"]), stage: Some("doing".into()),
        ..Default::default()
    });
    add(NewTask {
        title: "Call the landlord about the leak".into(),
        notes: Some("Water under the sink **again**. Photos of the damage below.\n\n> Ask when the plumber can come.".into()),
        severity: Some(3), start: Some(rel(25)), stop: Some(rel(55)), reminder_min: Some(10), tags: tags(&["home"]),
        photos: Some(photo_ids.clone()), ..Default::default()
    });
    add(NewTask { title: "Study chapter 6, linear algebra".into(), severity: Some(2), start: Some(at(0, 22, 0)), stop: Some(at(0, 23, 30)), repeat: Some("weekly".into()), list: Some(study.clone()), tags: tags(&["school"]), ..Default::default() });
    add(NewTask { title: "Pay rent".into(), severity: Some(4), start: Some(at(1, 17, 0)), reminder_min: Some(60), repeat: Some("monthly".into()), tags: tags(&["home", "money"]), ..Default::default() });
    add(NewTask { title: "Dentist".into(), severity: Some(3), start: Some(at(4, 15, 30)), stop: Some(at(4, 16, 15)), reminder_min: Some(120), tags: tags(&["health"]), ..Default::default() });
    add(NewTask { title: "Gym: legs".into(), severity: Some(2), start: Some(at(1, 7, 0)), stop: Some(at(1, 8, 0)), repeat: Some("weekdays".into()), tags: tags(&["health"]), ..Default::default() });
    add(NewTask { title: "Renew passport".into(), notes: Some("Needs:\n1. Photo\n2. Old passport\n3. Form `AP-1`".into()), severity: Some(4), start: Some(day(9)), tags: tags(&["admin"]), stage: Some("doing".into()), ..Default::default() });
    add(NewTask { title: "Mum's birthday".into(), severity: Some(3), start: Some(day(12)), reminder_min: Some(1440), repeat: Some("yearly".into()), ..Default::default() });
    add(NewTask { title: "Lab report: optics".into(), severity: Some(4), start: Some(at(6, 9, 0)), list: Some(study), tags: tags(&["school"]), ..Default::default() });
    add(NewTask { title: "Read Dune".into(), severity: Some(1), tags: tags(&["reading"]), ..Default::default() });
    add(NewTask { title: "Fix bike brakes".into(), severity: Some(2), ..Default::default() });
    let d1 = add(NewTask { title: "Reply to Sam".into(), severity: Some(2), ..Default::default() });
    tasks::toggle(&s, &d1.id, now)?;
    let d2 = add(NewTask { title: "Morning run".into(), severity: Some(3), start: Some(at(0, 6, 30)), tags: tags(&["health"]), ..Default::default() });
    tasks::toggle(&s, &d2.id, now)?;

    // ~4 months of history: completed tasks (records + history entries) and focus sessions.
    let titles = [("Weekly review", "admin"), ("Groceries", "home"), ("Problem set", "school"), ("Laundry", "home"), ("Email the tutor", "school"),
        ("Clean the kitchen", "home"), ("Read 30 pages", "reading"), ("Budget check", "money"), ("Physics notes", "school"), ("Call grandma", "")];
    let mut seed: u64 = 42;
    let mut rnd = |n: u64| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    for back in 1..=120i64 {
        let date = now.date() - Duration::days(back);
        let weekend = matches!(date.format("%a").to_string().as_str(), "Sat" | "Sun");
        let n = rnd(if weekend { 3 } else { 5 }) + if back < 30 { 1 } else { 0 };
        for _ in 0..n {
            let (title, tag) = titles[rnd(titles.len() as u64) as usize];
            let h = 8 + rnd(13) as u32;
            let done_local = date.and_hms_opt(h, rnd(60) as u32, 0).unwrap();
            let done_ms = Local.from_local_datetime(&done_local).earliest().unwrap().timestamp_millis();
            let dated = rnd(10) < 7;
            let late = rnd(10) < 2;
            let due = if late { done_local - Duration::hours(3 + rnd(30) as i64) } else { done_local + Duration::hours(1 + rnd(10) as i64) };
            let mut d = json!({"title": title, "severity": 2 + rnd(3), "list": tasks::INBOX, "created_at": done_ms - 86_400_000, "done_at": done_ms});
            let mut c = json!({"title": title, "done_at": done_ms, "tz_offset_min": selfkit_core::time::local_offset_min()});
            if dated {
                d["start"] = json!(due.format("%Y-%m-%dT%H:%M").to_string());
                c["due"] = d["start"].clone();
                c["on_time"] = json!(!late);
            }
            if !tag.is_empty() {
                d["tags"] = json!([tag]);
                c["tags"] = json!([tag]);
            }
            let t = s.put(tasks::TASKS, None, &d)?;
            c["task"] = json!(t.id);
            let cr = s.put(tasks::COMPLETIONS, None, &c)?;
            let mut d = d;
            d["last_completion"] = json!(cr.id);
            s.put(tasks::TASKS, Some(&t.id), &d)?;
        }
        for k in 0..rnd(if weekend { 2 } else { 5 }) {
            let st = date.and_hms_opt(9 + k as u32 * 2, 0, 0).unwrap();
            let start = Local.from_local_datetime(&st).earliest().unwrap().timestamp_millis();
            let full = rnd(10) < 8;
            let minutes = if full { 25 } else { 5 + rnd(18) as i64 };
            let (title, _) = titles[rnd(titles.len() as u64) as usize];
            s.put(focus::FOCUS, None, &json!({"title": title, "kind": "focus", "start": start, "end": start + minutes * 60_000, "minutes": minutes, "planned_min": 25, "completed": full, "tz_offset_min": 0}))?;
            if full {
                s.put(focus::FOCUS, None, &json!({"title": "Break", "kind": "break", "start": start + 25 * 60_000, "end": start + 30 * 60_000, "minutes": 5, "planned_min": 5, "completed": true, "tz_offset_min": 0}))?;
            }
        }
    }
    println!("seeded {}", paths::data_dir().display());
    Ok(())
}
