//! Sample data for Agenda (testing, screenshots). Times are relative to now.
//! `SELFKIT_DATA_DIR=/tmp/x cargo run -p agenda --example seed [-- --photo a.jpg --photo b.jpg]`
//! `--theme light|dark` only sets the app theme preference.
//! Adds: open tasks with tags, Markdown notes, steps, repeats, a Doing column,
//! photos (when `--photo` files are given), ~4 months of completed-task history
//! and focus sessions (for History and Stats).
//! `--year`: instead, a year of everyday use (QA, performance): ~1,000 finished
//! tasks, ~150 open ones (repeats, tags, steps, notes, photos, long/emoji/RTL
//! titles), daily habits with completions, and focus sessions for 365 days.

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
    if args.iter().any(|a| a == "--year") {
        year(&s, now, &photo_ids)?;
        println!("seeded a year into {}", paths::data_dir().display());
        return Ok(());
    }
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

/// A year of everyday use (see the file comment). Deterministic.
fn year(s: &Store, now: chrono::NaiveDateTime, photo_ids: &[String]) -> selfkit_core::Result<()> {
    let mut seed: u64 = 7;
    let mut rnd = |n: u64| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) % n.max(1)
    };
    let fmt = |d: chrono::NaiveDateTime| d.format("%Y-%m-%dT%H:%M").to_string();
    let ms = |d: chrono::NaiveDateTime| Local.from_local_datetime(&d).earliest().unwrap().timestamp_millis();
    let off = selfkit_core::time::local_offset_min();
    let lists = [tasks::INBOX.to_string(), tasks::add_list(s, "Study")?, tasks::add_list(s, "Work")?, tasks::add_list(s, "Home")?];
    let one_offs: [(&str, &str, usize); 24] = [
        ("Groceries", "home", 3), ("Laundry", "home", 3), ("Problem set", "school", 1), ("Email the tutor", "school", 1),
        ("Clean the kitchen", "home", 3), ("Budget check", "money", 0), ("Physics notes", "school", 1), ("Call grandma", "family", 0),
        ("Fix the CI pipeline", "work", 2), ("Review pull request", "work", 2), ("Team meeting notes", "work", 2), ("Book train tickets", "travel", 0),
        ("Return library books", "reading", 0), ("Buy a birthday present", "family", 0), ("Change bed sheets", "home", 3), ("Car service", "admin", 0),
        ("Pay electricity bill", "money", 3), ("Prepare slides", "work", 2), ("Lab report", "school", 1), ("Backup photos", "admin", 0),
        ("রিপোর্ট জমা দাও", "work", 2), ("مراجعة الملاحظات", "school", 1), ("Plan the weekend 🌲⛺", "", 0), ("Order new glasses 👓", "health", 0),
    ];
    let notes = ["Remember the receipt.", "**Important**: before noon.\n\n- first\n- second", "See the email from Monday.", "> Ask about the deadline.", ""];
    // Finished one-off tasks over the past 365 days.
    let mut done_n = 0;
    for back in 1..=365i64 {
        let date = now.date() - Duration::days(back);
        let weekend = matches!(date.format("%a").to_string().as_str(), "Sat" | "Sun");
        for _ in 0..(1 + rnd(if weekend { 3 } else { 4 })) {
            let (title, tag, li) = one_offs[rnd(one_offs.len() as u64) as usize];
            let done_local = date.and_hms_opt(7 + rnd(15) as u32, rnd(60) as u32, 0).unwrap();
            let done_ms = ms(done_local);
            let mut d = json!({"title": title, "severity": 1 + rnd(5), "list": lists[li], "created_at": done_ms - (1 + rnd(10) as i64) * 86_400_000, "done_at": done_ms});
            let mut c = json!({"title": title, "done_at": done_ms, "tz_offset_min": off});
            if rnd(10) < 7 {
                let late = rnd(10) < 2;
                let due = if late { done_local - Duration::hours(2 + rnd(48) as i64) } else { done_local + Duration::hours(1 + rnd(12) as i64) };
                d["start"] = json!(fmt(due));
                c["due"] = d["start"].clone();
                c["on_time"] = json!(!late);
            }
            if !tag.is_empty() {
                d["tags"] = json!([tag]);
                c["tags"] = json!([tag]);
            }
            let n = notes[rnd(notes.len() as u64) as usize];
            if !n.is_empty() {
                d["notes"] = json!(n);
            }
            if rnd(6) == 0 {
                d["steps"] = json!([{"id":"a","title":"Start","done":true},{"id":"b","title":"Finish","done":true}]);
            }
            let t = s.put(tasks::TASKS, None, &d)?;
            c["task"] = json!(t.id);
            let cr = s.put(tasks::COMPLETIONS, None, &c)?;
            d["last_completion"] = json!(cr.id);
            s.put(tasks::TASKS, Some(&t.id), &d)?;
            done_n += 1;
        }
    }
    // Repeating habits: one open record each, a completion for most past occurrences.
    let habits: [(&str, &str, &str, u32, &str); 9] = [
        ("Take vitamins", "daily", "health", 8, ""), ("Gym", "weekdays", "health", 7, "Warm up, 3 sets, stretch"), ("Weekly review", "weekly", "admin", 18, "- Inbox zero\n- Plan next week"),
        ("Pay rent", "monthly", "money", 17, ""), ("Water the plants", "weekly", "home", 9, ""), ("Read 30 pages", "daily", "reading", 22, ""),
        ("Study linear algebra", "weekly", "school", 20, ""), ("Mum's birthday 🎂", "yearly", "family", 9, ""), ("Call grandma", "weekly", "family", 19, ""),
    ];
    let mut comp_n = 0;
    for (title, rep, tag, h, note) in habits {
        let first = now.date() - Duration::days(365);
        let mut day = first;
        let mut last_c = None;
        while day < now.date() {
            let hit = match rep {
                "daily" => true,
                "weekdays" => !matches!(day.format("%a").to_string().as_str(), "Sat" | "Sun"),
                "weekly" => (now.date() - day).num_days() % 7 == 0,
                "monthly" => day.format("%d").to_string() == "01",
                _ => (now.date() - day).num_days() == 360,
            };
            if hit && rnd(10) < 8 {
                let due = day.and_hms_opt(h, 0, 0).unwrap();
                let late = rnd(10) < 2;
                let done = if late { due + Duration::hours(3) } else { due - Duration::minutes(10 + rnd(50) as i64) };
                let c = s.put(tasks::COMPLETIONS, None, &json!({"title": title, "done_at": ms(done), "tz_offset_min": off, "due": fmt(due), "on_time": !late, "tags": [tag]}))?;
                last_c = Some((c.id, ms(done)));
                comp_n += 1;
            }
            day += Duration::days(1);
        }
        let next = match rep {
            "monthly" => (now.date() + Duration::days(20)).and_hms_opt(h, 0, 0).unwrap(),
            "yearly" => (now.date() + Duration::days(5)).and_hms_opt(h, 0, 0).unwrap(),
            _ => (now.date() + Duration::days(1)).and_hms_opt(h, 0, 0).unwrap(),
        };
        let mut d = json!({"title": title, "severity": 3, "list": tasks::INBOX, "start": fmt(next), "repeat": rep, "reminder_min": 15, "tags": [tag], "created_at": ms(first.and_hms_opt(8, 0, 0).unwrap())});
        if !note.is_empty() {
            d["notes"] = json!(note);
        }
        let t = s.put(tasks::TASKS, None, &d)?;
        if let Some((cid, at)) = last_c {
            d["last_completion"] = json!(cid);
            d["last_done_at"] = json!(at);
            s.put(tasks::TASKS, Some(&t.id), &d)?;
        }
    }
    // Open tasks: future, overdue, undated, Doing, with steps/photos/long text.
    let open_titles = ["Renew passport", "Write thesis chapter", "Fix bike brakes", "Dentist", "Sell old phone", "Plan trip to Sylhet", "Tax return",
        "Learn Rust lifetimes", "Paint the bedroom", "Update CV", "Hackathon idea", "Visit Nana", "Repair the fan", "Organise photos", "Draft blog post"];
    let tag_pool = ["school", "work", "home", "money", "health", "admin", "travel", "reading", "family", "ideas"];
    let mut open_n = 0;
    for i in 0..150u64 {
        let base = open_titles[(i as usize) % open_titles.len()];
        let title = match i {
            3 => "A very long task title that goes on and on to check how the rows wrap when someone writes a whole sentence as the title of a task instead of a short name".to_string(),
            7 => "مراجعة الفصل الثالث قبل الامتحان".to_string(),
            11 => "বাজার করা: চাল, ডাল, তেল 🛒".to_string(),
            13 => "🎉🎉 Party prep 🎈🍰🥳".to_string(),
            _ => format!("{base} #{}", i / open_titles.len() as u64 + 1),
        };
        let mut d = json!({"title": title, "severity": 1 + rnd(5), "list": lists[rnd(4) as usize], "created_at": ms(now - Duration::days(rnd(60) as i64))});
        let kind = rnd(10);
        if kind < 5 {
            let when = (now.date() + Duration::days(rnd(120) as i64)).and_hms_opt(8 + rnd(12) as u32, [0, 15, 30, 45][rnd(4) as usize], 0).unwrap();
            if rnd(4) == 0 { d["start"] = json!(when.format("%Y-%m-%d").to_string()); } else { d["start"] = json!(fmt(when)); d["stop"] = json!(fmt(when + Duration::minutes(30 + 30 * rnd(4) as i64))); }
            if rnd(2) == 0 { d["reminder_min"] = json!([0, 10, 30, 60, 1440][rnd(5) as usize]); }
        } else if kind < 7 {
            let when = (now.date() - Duration::days(1 + rnd(40) as i64)).and_hms_opt(9 + rnd(8) as u32, 0, 0).unwrap();
            d["start"] = json!(fmt(when));
        }
        let ntag = rnd(3);
        let mut tg: Vec<&str> = (0..ntag).map(|_| tag_pool[rnd(tag_pool.len() as u64) as usize]).collect();
        tg.dedup();
        if !tg.is_empty() { d["tags"] = json!(tg); }
        if rnd(4) == 0 {
            let n = 2 + rnd(6);
            d["steps"] = json!((0..n).map(|k| json!({"id": format!("s{k}"), "title": format!("Step {}", k + 1), "done": rnd(2) == 0})).collect::<Vec<_>>());
        }
        if rnd(3) == 0 { d["notes"] = json!(notes[rnd(4) as usize]); }
        if i == 5 { d["notes"] = json!("Long notes. ".repeat(400)); }
        if rnd(12) == 0 { d["stage"] = json!("doing"); }
        if !photo_ids.is_empty() && rnd(7) == 0 {
            let k = 1 + rnd(3) as usize;
            d["photos"] = json!((0..k).map(|j| photo_ids[(i as usize + j) % photo_ids.len()].clone()).collect::<Vec<_>>());
        }
        s.put(tasks::TASKS, None, &d)?;
        open_n += 1;
    }
    // Focus sessions for the year.
    let mut focus_n = 0;
    for back in 1..=365i64 {
        let date = now.date() - Duration::days(back);
        for k in 0..rnd(5) {
            let start = ms(date.and_hms_opt(9 + k as u32 * 2, 0, 0).unwrap());
            let full = rnd(10) < 8;
            let minutes = if full { 25 } else { 5 + rnd(18) as i64 };
            s.put(focus::FOCUS, None, &json!({"title": one_offs[rnd(20) as usize].0, "kind": "focus", "start": start, "end": start + minutes * 60_000, "minutes": minutes, "planned_min": 25, "completed": full, "tz_offset_min": off}))?;
            focus_n += 1;
        }
    }
    println!("year: {done_n} finished, {open_n} open, 9 habits with {comp_n} completions, {focus_n} focus sessions, {} photos", photo_ids.len());
    Ok(())
}
