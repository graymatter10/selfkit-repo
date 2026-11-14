//! Sample data for Reflect (testing, screenshots).
//! Run the other apps' seeds first into the same folder, then this:
//! ```sh
//! export SELFKIT_DATA_DIR=/tmp/reflect-demo
//! cargo run -p chrono-app --example seed -- --days 365
//! cargo run -p hisab --example seed
//! cargo run -p agenda --example seed -- --year
//! cargo run -p reflect --example reflect_seed [-- --days 365] [--health ok|warn|error|stale|none] [--theme light|dark]
//! ```
//! This writes what Dusk would: `dusk.db` with the `dusk_nights` cache (and the
//! matching `sleep_log` ratings), estimated from Chrono's phone sessions so the
//! connections have real patterns (late phone use → later bedtime, shorter
//! and worse sleep). It also writes a sample agent `health.json`.
//! `--theme X --theme-only` only sets the theme; `--health-only` only rewrites health.json; `--no-late` skips the late-night sessions. `--health` picks the health state (default warn = server DB 82 % full and
//! one open error). `--no-dusk` leaves Dusk out (empty-state testing).

use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
use reflect_lib::{model, sources};
use selfkit_core::store::Migration;
use selfkit_core::{Store, paths};
use serde_json::json;

const MIN: i64 = 60_000;

fn rnd(seed: &mut u64) -> f64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed % 10_000) as f64 / 10_000.0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("SELFKIT_DATA_DIR").is_err() {
        return Err("set SELFKIT_DATA_DIR to a demo folder".into());
    }
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let days: i64 = arg("--days").and_then(|x| x.parse().ok()).unwrap_or(365);
    let device = paths::device_id()?;
    let now = selfkit_core::time::now_ms();

    if let Some(t) = arg("--theme") {
        let r = Store::open(paths::app_db("reflect"), "reflect", &device, &[])?;
        r.kv_set(reflect_lib::PREFS_KV, &json!({"theme": t}).to_string())?;
        if args.iter().any(|a| a == "--theme-only") {
            return Ok(());
        }
    }

    // Late-night phone use (Chrono's seed has almost none): on ~1 in 4 nights add
    // 15–100 min of phone after midnight, as the Chrono importer would.
    // Marked with `"seed":"reflect-late"` so a re-run replaces them.
    let health_only = args.iter().any(|a| a == "--health-only");
    if !health_only && !args.iter().any(|a| a == "--no-late")
        && let Ok(c) = Store::open(paths::app_db("chrono"), "chrono", &device, &[])
    {
        let dev: String = c
            .conn()
            .query_row("SELECT json_extract(data,'$.device') FROM records WHERE collection='app_sessions' AND json_extract(data,'$.platform')='android' LIMIT 1", [], |r| r.get(0))
            .unwrap_or_else(|_| "phone-seed".into());
        let old: Vec<String> = c
            .conn()
            .prepare("SELECT id FROM records WHERE collection='app_sessions' AND deleted=0 AND json_extract(data,'$.seed')='reflect-late'")?
            .query_map([], |r| r.get(0))?
            .flatten()
            .collect();
        for id in old {
            c.delete(&id)?;
        }
        let mut seed = 0x51_7CC1_B727_220Au64;
        let tz = selfkit_core::time::local_offset_min();
        let today = Local::now().date_naive();
        let mut n = 0;
        for k in 1..days {
            let d = today - Duration::days(k);
            let wkend = matches!(d.weekday().num_days_from_monday(), 5 | 6);
            if rnd(&mut seed) > if wkend { 0.45 } else { 0.18 } {
                continue;
            }
            let mut t = Local.from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap()).earliest().unwrap().timestamp_millis() + (rnd(&mut seed) * 40.0) as i64 * MIN;
            let total = 15.0 + rnd(&mut seed) * 85.0;
            let mut used = 0.0;
            while used < total {
                let len = (4.0 + rnd(&mut seed) * 18.0).min(total - used + 1.0);
                let app = ["YouTube", "Instagram", "Reddit", "WhatsApp"][(rnd(&mut seed) * 4.0) as usize % 4];
                let e = t + (len * MIN as f64) as i64;
                c.put("app_sessions", None, &json!({"app": app, "platform": "android", "package": format!("com.example.{}", app.to_lowercase()), "device": dev, "start": t, "end": e, "tz_offset": tz, "seed": "reflect-late"}))?;
                used += len;
                t = e + (1.0 + rnd(&mut seed) * 4.0) as i64 * MIN;
            }
            n += 1;
        }
        println!("late-night phone use added on {n} nights");
    }

    if !health_only && !args.iter().any(|a| a == "--no-dusk") {
        // Phone sessions from Chrono (all of them, once).
        let phone: Vec<sources::Sess> = sources::open_other("chrono")
            .map(|s| sources::sessions_between(&s, 0, i64::MAX).into_iter().filter(|x| x.phone).collect())
            .unwrap_or_default();
        println!("chrono phone sessions: {}", phone.len());
        let _ = std::fs::remove_file(paths::app_db("dusk"));
        const MIGS: &[Migration] = &[Migration {
            name: "dusk/001_nights",
            sql: "CREATE TABLE IF NOT EXISTS dusk_nights (
            night TEXT PRIMARY KEY, bed INTEGER, wake INTEGER, tz_offset INTEGER NOT NULL,
            asleep_min INTEGER NOT NULL, nap_min INTEGER NOT NULL, quality INTEGER,
            source TEXT NOT NULL, late_use_min INTEGER NOT NULL, updated_at INTEGER NOT NULL);",
        }];
        let dusk = Store::open(paths::app_db("dusk"), "dusk", &device, MIGS)?;
        let today = Local::now().date_naive();
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let tz = selfkit_core::time::local_offset_min();
        let at = |d: NaiveDate, h: u32, m: u32| Local.from_local_datetime(&d.and_hms_opt(h, m, 0).unwrap()).earliest().unwrap().timestamp_millis();
        let tx = dusk.conn().unchecked_transaction()?;
        for k in (0..days).rev() {
            let night = today - Duration::days(k);
            let eve = night - Duration::days(1);
            let wkend = matches!(night.weekday().num_days_from_monday(), 5 | 6);
            // Last phone use between 21:00 and 04:00 → bedtime.
            let (w0, w1) = (at(eve, 21, 0), at(night, 4, 0));
            let last_use = phone.iter().filter(|s| s.end > w0 && s.start < w1).map(|s| s.end.min(w1)).max();
            let late: i64 = phone.iter().map(|s| (s.end.min(w1) - s.start.max(at(night, 0, 0))).max(0)).sum::<i64>() / MIN;
            let base_bed = at(eve, 23, 5) + ((rnd(&mut seed) * 50.0) as i64 + if wkend { 35 } else { 0 }) * MIN;
            let bed = last_use.map_or(base_bed, |u| base_bed.max(u + (8.0 + rnd(&mut seed) * 15.0) as i64 * MIN));
            let wake = if wkend { at(night, 7, 50) } else { at(night, 6, 50) } + ((rnd(&mut seed) * 45.0) as i64 - 10) * MIN;
            let wake = wake.max(bed + 4 * 60 * MIN);
            let awake = (5.0 + rnd(&mut seed) * 20.0 + late as f64 * 0.15) as i64;
            let asleep = ((wake - bed) / MIN - awake).max(0);
            if k == 0 && Local::now().timestamp_millis() < wake {
                continue; // not over yet
            }
            let logged = rnd(&mut seed) < 0.12;
            let rated = rnd(&mut seed) < 0.6;
            let q = (2.2 + (asleep as f64 - 360.0) / 70.0 - late as f64 / 45.0 + rnd(&mut seed) * 1.4).round().clamp(1.0, 5.0) as i64;
            let quality = rated.then_some(q);
            let key = night.format("%Y-%m-%d").to_string();
            tx.execute(
                "INSERT OR REPLACE INTO dusk_nights (night, bed, wake, tz_offset, asleep_min, nap_min, quality, source, late_use_min, updated_at) VALUES (?1,?2,?3,?4,?5,0,?6,?7,?8,?9)",
                (&key, bed, wake, tz, asleep, quality, if logged { "logged" } else { "estimate" }, late, now),
            )?;
            if rated || logged {
                dusk.put(
                    "sleep_log",
                    Some(&format!("n-{key}")),
                    &json!({"night": key, "kind": "main", "bed": logged.then_some(bed), "wake": logged.then_some(wake), "tz_offset": tz, "quality": quality, "note_before": null, "note_after": null, "created_at": now}),
                )?;
            }
        }
        tx.commit()?;
        println!("dusk nights: {}", dusk.conn().query_row("SELECT COUNT(*) FROM dusk_nights", [], |r| r.get::<_, i64>(0))?);
    }

    // Agent health.json
    let state = arg("--health").unwrap_or_else(|| "warn".into());
    let hp = paths::data_dir().join("health.json");
    if state == "none" {
        let _ = std::fs::remove_file(&hp);
    } else {
        let mb = 1024 * 1024;
        let written = if state == "stale" { now - 3 * 3600 * 1000 } else { now - 20_000 };
        let (db, warn) = if state == "ok" { (210 * mb, false) } else { (420 * mb, true) };
        let mut errors = vec![json!({"at": now - 2 * 86_400_000 - 5 * 3_600_000, "level": "WARN", "source": "selfkit_agent::backup", "message": "upload failed: network unreachable (retrying)", "pid": 4121})];
        let mut tracking_err = json!(null);
        if state == "error" {
            errors.insert(0, json!({"at": now - 2 * 3_600_000, "level": "ERROR", "source": "selfkit_agent::sync_loop", "message": "server sync failed: connection refused (postgres)", "pid": 4121}));
        }
        if state == "error" {
            tracking_err = json!("X display not available");
        }
        let doc = json!({
            "schema": 1, "written_at": written, "version": "0.1.0", "device_id": device,
            "agent": {"pid": 4121, "started_at": now - 3 * 86_400_000, "uptime_secs": 3 * 86_400, "rss_kib": 25_600, "cpu_percent": 0.2, "cpu_secs_total": 512.0},
            "tracking": {"on": state != "error", "paused": false, "paused_until": null, "current_app": "firefox", "excluded_apps": 1, "error": tracking_err},
            "web": {"last_session_end": {"firefox": now - 60_000}},
            "sync": {"configured": true, "last_at": now - 3 * MIN, "last_ok_at": if state == "error" { now - 2 * 3_600_000 - 5 * MIN } else { now - 3 * MIN },
                     "last_error": if state == "error" { json!("connection refused (postgres)") } else { json!(null) },
                     "last_report": {"pulled": 12, "pushed": 40, "conflicts": 0, "undecryptable": 0},
                     "pending_records": if state == "ok" { 0 } else { 214 }, "server_db_bytes": db, "server_db_limit_bytes": 512 * mb, "server_db_warn": warn},
            "lan": {"enabled": true, "error": null, "peers": 1},
            "backup": {"enabled": true, "keep_local": false, "target": "s3", "last_run_at": now - 5 * 3_600_000, "last_ok_at": now - 5 * 3_600_000, "last_error": null,
                       "last_weekly": format!("{}-W{:02}", Local::now().iso_week().year(), Local::now().iso_week().week()), "last_monthly": {"chrono": "2026-10", "hisab": "2026-10"}},
            "reminders": {"enabled": true, "last_fired_at": null, "fired_since_start": 2},
            "storage": {"data_dir": paths::data_dir().display().to_string(), "total_bytes": 186 * mb,
                        "apps": {"chrono": 96 * mb, "hisab": 31 * mb, "agenda": 22 * mb, "reflect": 3 * mb / 10, "dusk": 12 * mb, "settings": mb / 10},
                        "files_bytes": 18 * mb, "backups_bytes": 4 * mb, "logs_bytes": mb / 2},
            "errors": errors,
        });
        std::fs::write(&hp, serde_json::to_string_pretty(&doc)?)?;
        println!("health.json: {state}");
    }
    let _ = model::DAY;
    Ok(())
}
