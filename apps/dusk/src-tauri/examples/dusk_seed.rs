//! Sample data for Dusk (testing, screenshots): ~4 months of nights relative to now.
//! `SELFKIT_DATA_DIR=/tmp/dusk-demo cargo run -p dusk --example dusk_seed`
//! Writes dusk.db (ratings, corrections, naps, notes, phone intervals), and,
//! as the other apps would: chrono.db (desktop sessions), agenda.db (tasks done)
//! and hisab.db (expenses). The data has real patterns: late screen use →
//! worse ratings, short nights → fewer tasks and more spending.
//! `--theme light|dark` only sets the theme. `--days N` (default 120).

use chrono::{Duration, Local, NaiveTime};
use dusk_lib::data;
use dusk_lib::model::{self, MIN, Ms};
use selfkit_core::{Store, paths};
use serde_json::json;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f64) / ((1u64 << 31) as f64)
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
    fn chance(&mut self, p: f64) -> bool {
        self.next() < p
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("SELFKIT_DATA_DIR").is_err() {
        return Err("set SELFKIT_DATA_DIR to a demo folder (the seed replaces dusk/chrono/agenda/hisab DBs there)".into());
    }
    let args: Vec<String> = std::env::args().collect();
    let device = paths::device_id()?;
    if let Some(i) = args.iter().position(|a| a == "--theme") {
        let s = Store::open(paths::app_db("dusk"), "dusk", &device, data::MIGRATIONS)?;
        let mut p = data::device(&s)?;
        p.theme = args[i + 1].clone();
        data::save_device(&s, &p)?;
        return Ok(());
    }
    let days: i64 = args.iter().position(|a| a == "--days").and_then(|i| args.get(i + 1)?.parse().ok()).unwrap_or(120);
    for app in ["dusk", "chrono", "agenda", "hisab"] {
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", paths::app_db(app).display()));
        }
    }
    let dusk = Store::open(paths::app_db("dusk"), "dusk", &device, data::MIGRATIONS)?;
    let chrono = Store::open(paths::app_db("chrono"), "chrono", &device, &[])?;
    let agenda = Store::open(paths::app_db("agenda"), "agenda", &device, &[])?;
    let hisab = Store::open(paths::app_db("hisab"), "hisab", &device, &[])?;
    let tz_here = selfkit_core::time::local_offset_min();
    let now = selfkit_core::time::now_ms();
    let today = Local::now().date_naive();
    let mut rng = Rng(42);
    let phone = "seed-phone";
    let t = |h: u32, m: u32| NaiveTime::from_hms_opt(h, m, 0).unwrap();

    for k in (0..days).rev() {
        let night = today - Duration::days(k);
        let key = model::date_str(night);
        // A week abroad (3 h ahead) about two months ago.
        let tz = if (58..65).contains(&k) { tz_here + 180 } else { tz_here };
        let at = |d: chrono::NaiveDate, h: u32, m: u32| model::local_to_utc(d, t(h, m), tz);
        let eve = night - Duration::days(1);
        let wd = chrono::Datelike::weekday(&night).num_days_from_monday(); // morning weekday
        let weekend = wd >= 5;
        // Bedtime (minutes from midnight; negative = evening) and sleep length.
        let late_night = rng.chance(if weekend { 0.45 } else { 0.22 });
        let bed_rel = if late_night { rng.range(20.0, 140.0) } else { rng.range(-75.0, 5.0) } as i64;
        let mut asleep = if weekend { rng.range(450.0, 560.0) } else { rng.range(370.0, 470.0) } as i64;
        if late_night && !weekend {
            asleep -= 40;
        }
        let bed = at(night, 0, 0) + bed_rel * MIN;
        let wake = (bed + asleep * MIN).min(at(night, 11, 30));
        let allnighter = k == 41;

        // Activity: phone + desktop sessions in the evening up to bed, and after waking.
        let mut phone_iv: Vec<(Ms, Ms)> = vec![];
        let mut desk: Vec<(Ms, Ms)> = vec![];
        let mut cur = at(eve, 18, 30);
        let evening_end = if allnighter { at(night, 9, 0) } else { bed };
        while cur < evening_end {
            let len = rng.range(3.0, 14.0) as i64 * MIN;
            let end = (cur + len).min(evening_end);
            let on_desktop = model::local_minute(cur, tz) < 23 * 60 && model::local_minute(cur, tz) >= 18 * 60 && rng.chance(0.55);
            if on_desktop { desk.push((cur, end)) } else { phone_iv.push((cur, end)) }
            cur = end + rng.range(1.0, 12.0) as i64 * MIN;
        }
        // A quick look at the phone in the night.
        if !allnighter && rng.chance(0.25) {
            let mid = bed + (wake - bed) / 2;
            phone_iv.push((mid, mid + rng.range(2.0, 6.0) as i64 * MIN));
        }
        if k > 0 || now > wake {
            let mut cur = if allnighter { at(night, 9, 0) } else { wake };
            let until = at(night, 17, 30).min(now);
            while cur < until {
                let len = rng.range(4.0, 25.0) as i64 * MIN;
                let end = (cur + len).min(until);
                if rng.chance(0.5) { desk.push((cur, end)) } else { phone_iv.push((cur, end)) }
                cur = end + rng.range(5.0, 45.0) as i64 * MIN;
            }
        }
        dusk_lib::sources::save_device_use(&dusk, phone, tz, &phone_iv.iter().copied().filter(|(s, _)| *s < now).collect::<Vec<_>>())?;
        for (s, e) in desk.into_iter().filter(|(s, _)| *s < now) {
            let app = ["firefox", "code", "Alacritty", "obsidian"][(rng.next() * 4.0) as usize % 4];
            chrono.put("app_sessions", None, &json!({"app": app, "platform": "linux", "start": s, "end": e.min(now), "tz_offset": tz}))?;
        }

        // How it felt: late screen use and short nights are rated lower.
        let late_min = if allnighter { 300 } else { bed_rel.max(0) };
        let q = (4.4 - late_min as f64 / 45.0 - ((470 - asleep).max(0) as f64) / 90.0 + rng.range(-0.7, 0.7)).round().clamp(1.0, 5.0) as u8;
        let rated = k > 0 && rng.chance(0.82);
        let corrected = k > 0 && !allnighter && rng.chance(0.08);
        let mut entry = data::log_json(&key, "main", None, None, tz, rated.then_some(q));
        if corrected {
            entry["bed"] = json!(bed - 20 * MIN);
            entry["wake"] = json!(wake - 5 * MIN);
        }
        if k > 0 && rng.chance(0.18) {
            let notes = ["Coffee after 6 pm", "Late dinner", "Read a book in bed", "Gym in the evening", "Worried about the exam", "Hot room"];
            entry["note_before"] = json!(notes[(rng.next() * notes.len() as f64) as usize % notes.len()]);
        }
        if k > 0 && rng.chance(0.1) {
            let notes = ["Vivid dreams", "Woke up groggy", "Felt rested", "Headache"];
            entry["note_after"] = json!(notes[(rng.next() * notes.len() as f64) as usize % notes.len()]);
        }
        if rated || corrected || entry["note_before"].is_string() || entry["note_after"].is_string() {
            dusk.put(data::LOG, Some(&data::main_id(&key)), &entry)?;
        }
        if k > 0 && (rng.chance(0.07) || allnighter) {
            let s = at(night, 14, rng.range(0.0, 40.0) as u32);
            let e = s + if allnighter { 150 } else { rng.range(20.0, 55.0) as i64 } * MIN;
            dusk.put(data::LOG, None, &data::log_json(&key, "nap", Some(s), Some(e), tz, None))?;
        }

        // The day after: tasks done (Agenda) and money spent (Hisab).
        if k > 0 {
            let slept = if allnighter { 0 } else { asleep };
            let n = (0.5 + 4.0 * (slept as f64 / 480.0).powi(2) + rng.range(-1.2, 1.2)).round().max(0.0) as i64;
            for i in 0..n {
                let done = at(night, 9 + (i as u32 * 2) % 12, rng.range(0.0, 59.0) as u32);
                agenda.put("tasks", None, &json!({"title": format!("Sample task {key} #{i}"), "severity": 3, "done_at": done, "created_at": done - 2 * 86_400_000}))?;
            }
            let spend = (24.0 + (480 - slept).max(0) as f64 / 7.0 + rng.range(-12.0, 18.0)).max(0.0);
            let parts = 1 + (rng.next() * 3.0) as i64;
            for i in 0..parts {
                hisab.put(
                    "transactions",
                    None,
                    &json!({"kind": "expense", "amount": (spend / parts as f64 * 100.0).round() / 100.0, "currency": "USD",
                            "amount_main": (spend / parts as f64 * 100.0).round() / 100.0, "date": key,
                            "time": format!("{:02}:{:02}", 10 + i * 3, 15), "account": "cash", "category": "food"}),
                )?;
            }
            if rng.chance(0.1) {
                hisab.put("transactions", None, &json!({"kind": "income", "amount": 900, "amount_main": 900, "date": key, "time": "12:00", "account": "bank"}))?;
            }
        }
    }
    let _ = data::save_goals(&dusk, &data::GoalsRec::default());
    // Shared settings: a new day starts at 04:00 (so "last night" is complete at 2 am).
    let settings = Store::open(paths::app_db(selfkit_core::settings::APP), selfkit_core::settings::APP, &device, &[])?;
    let mut st = selfkit_core::settings::Settings::load(&settings)?;
    st.day_start_min = 240;
    st.save(&settings)?;
    println!("seeded {days} nights into {}", paths::data_dir().display());
    Ok(())
}
