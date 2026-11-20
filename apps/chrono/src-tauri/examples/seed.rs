//! Sample data for Chrono: ~8 months of realistic activity on three devices
//! (this Linux PC, a Windows PC, an Android phone) plus browser time, hourly
//! phone counters, settings with goals. Times are relative to now.
//! `SELFKIT_DATA_DIR=/tmp/x cargo run -p chrono-app --example seed [-- --days 240] [--theme dark] [--settings-only]`

use chrono::{Duration, Local, TimeZone, Timelike};
use chrono_lib::settings::{CategoryRule, ChronoSettings, Goal};
use selfkit_core::time;
use serde_json::json;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0
    }
    fn range(&mut self, a: i64, b: i64) -> i64 {
        a + (self.next() % ((b - a).max(1) as u64)) as i64
    }
    fn pick<'a, T>(&mut self, v: &'a [(T, f64)]) -> &'a T {
        let tot: f64 = v.iter().map(|x| x.1).sum();
        let mut r = self.f() * tot;
        for (x, w) in v {
            if r < *w {
                return x;
            }
            r -= w;
        }
        &v[v.len() - 1].0
    }
}

const M: i64 = 60_000;

fn main() -> selfkit_core::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let s = chrono_lib::open_store()?;
    if let Some(t) = arg("--theme") {
        s.kv_set("chrono.prefs", &json!({"theme": t}).to_string())?;
        if args.len() <= 3 {
            return Ok(());
        }
    }
    let pc = s.device_id().to_string();
    let win = "0199aa00-win0-7000-8000-00000000beef".to_string();
    let phone = "0199aa00-phon-7000-8000-00000000cafe".to_string();

    let mut set = ChronoSettings::load(&s)?;
    set.goals = vec![
        Goal { id: "lim-ent".into(), kind: "limit".into(), scope: "category".into(), target: "entertainment".into(), minutes: 90, enabled: true },
        Goal { id: "goal-study".into(), kind: "goal".into(), scope: "category".into(), target: "study".into(), minutes: 45, enabled: true },
        Goal { id: "lim-total".into(), kind: "limit".into(), scope: "total".into(), target: String::new(), minutes: 8 * 60, enabled: true },
    ];
    set.device_names.insert(pc.clone(), "Linux desktop".into());
    set.device_names.insert(win.clone(), "Gaming PC".into());
    set.device_names.insert(phone.clone(), "Pixel".into());
    set.categories.insert("Obsidian".into(), "study".into());
    set.exclude_apps = vec!["keepassxc".into()];
    // Time rules (Websites / Settings → Categories screenshots): YouTube is Work in office hours,
    // Telegram is Work late on Fridays (overnight), one rule switched off.
    set.category_rules = vec![
        CategoryRule { id: "r-yt".into(), target: "youtube.com".into(), kind: "domain".into(), category: "work".into(), days: vec![1, 2, 3, 4, 5], start_min: 9 * 60, end_min: 17 * 60, enabled: true },
        CategoryRule { id: "r-tg".into(), target: "Telegram".into(), kind: "app".into(), category: "work".into(), days: vec![5], start_min: 22 * 60, end_min: 2 * 60, enabled: true },
        CategoryRule { id: "r-rd".into(), target: "reddit.com".into(), kind: "domain".into(), category: "study".into(), days: vec![6, 7], start_min: 10 * 60, end_min: 12 * 60, enabled: false },
    ];
    set.save(&s)?;
    if args.iter().any(|a| a == "--bench") {
        // cold (empty cache) and warm timings of the heavy screens
        s.conn().execute("DELETE FROM chrono_daily", [])?;
        let now = time::now_ms();
        let e = chrono_lib::stats::Engine::new(&s, set.clone(), 0, time::local_offset_min());
        let today = e.today(now);
        for round in ["cold", "warm"] {
            let t = std::time::Instant::now();
            let l = chrono_lib::stats::trends(&e, "lifetime", today, now)?;
            let a = t.elapsed();
            let t = std::time::Instant::now();
            chrono_lib::stats::trends(&e, "year", today, now)?;
            chrono_lib::stats::day_view(&e, today, now)?;
            chrono_lib::stats::report(&e, today, now)?;
            chrono_lib::stats::timeline(&e, today)?;
            println!("{round}: lifetime {:?} ({} buckets, {} apps), year+today+report+timeline {:?}", a, l.buckets.len(), l.apps.len(), t.elapsed());
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--settings-only") {
        return Ok(());
    }

    let days: i64 = arg("--days").and_then(|d| d.parse().ok()).unwrap_or(240);
    let now = time::now_ms();
    let tz = time::local_offset_min() as i64;
    let mut r = Rng(0x9E3779B97F4A7C15);
    let c = s.conn();
    c.execute_batch("BEGIN")?;
    let n = std::cell::Cell::new(0usize);
    let put = |col: &str, id: Option<&str>, v: serde_json::Value| {
        s.put(col, id, &v).unwrap();
        n.set(n.get() + 1);
    };
    let labels = [
        ("com.whatsapp", "WhatsApp"), ("com.instagram.android", "Instagram"), ("com.google.android.youtube", "YouTube"),
        ("com.android.chrome", "Chrome"), ("com.google.android.gm", "Gmail"), ("com.duolingo", "Duolingo"),
        ("com.ichi2.anki", "AnkiDroid"), ("com.spotify.music", "Spotify"), ("org.telegram.messenger", "Telegram"),
        ("com.google.android.apps.maps", "Maps"), ("com.selfkit.agenda", "Agenda"), ("com.reddit.frontpage", "Reddit"),
        ("com.google.android.calendar", "Calendar"), ("com.google.android.GoogleCamera", "Camera"), ("com.zhiliaoapp.musically", "TikTok"),
    ];
    for (p, l) in labels {
        put("android_apps", Some(&format!("p-{p}")), json!({"package": p, "label": l}));
    }
    let label = |p: &str| labels.iter().find(|x| x.0 == p).map(|x| x.1).unwrap_or(p).to_string();

    let today = Local::now().date_naive();
    for back in (0..days).rev() {
        let date = today - Duration::days(back);
        let wd = date.and_hms_opt(12, 0, 0).unwrap().and_local_timezone(Local).unwrap().format("%u").to_string().parse::<i64>().unwrap();
        let weekend = wd >= 6;
        let mid = Local.from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap()).earliest().unwrap().timestamp_millis();
        let at = |h: f64| mid + (h * 60.0) as i64 * M;
        // slow drift: study grows, social shrinks over the months
        let drift = 1.0 - back as f64 / days as f64;

        // ---- Linux desktop: work blocks with switches + web
        let blocks: Vec<(f64, f64)> = if weekend {
            if r.f() < 0.6 { vec![(10.5 + r.f(), 12.5 + r.f()), (20.0 + r.f(), 22.5 + r.f())] } else { vec![(21.0, 22.0 + r.f())] }
        } else if r.f() < 0.06 {
            vec![] // day off
        } else {
            let mut v = vec![(9.0 + r.f() * 0.6, 12.0 + r.f() * 0.4), (13.2 + r.f() * 0.4, 16.8 + r.f())];
            if r.f() < 0.55 {
                v.push((20.5 + r.f(), 21.6 + r.f() * 1.2));
            }
            v
        };
        for (bi, (bs, be)) in blocks.iter().enumerate() {
            let evening = bi == 2 || *bs >= 20.0;
            let mut t = at(*bs);
            let end = at(*be);
            while t < end {
                let app: &str = if evening {
                    r.pick(&[("firefox", 4.0), ("spotify", 1.0), ("steam", 0.6), ("discord", 1.2), ("obsidian", 0.8 + drift), ("anki", 0.5 + drift)])
                } else {
                    r.pick(&[("code", 5.0), ("alacritty", 2.5), ("firefox", 3.5), ("obsidian", 0.6), ("thunderbird", 0.5), ("slack", 0.8), ("keepassxc", 0.1)])
                };
                let len = match app {
                    "code" => r.range(4 * M, 40 * M),
                    "firefox" => r.range(3 * M, 25 * M),
                    "steam" => r.range(30 * M, 90 * M),
                    "anki" | "obsidian" => r.range(10 * M, 35 * M),
                    _ => r.range(M / 2, 8 * M),
                };
                let e = (t + len).min(end);
                if now < e {
                    break;
                }
                put("app_sessions", None, json!({"app": app, "platform": "linux", "start": t, "end": e, "tz_offset": tz}));
                if app == "firefox" {
                    let mut w = t;
                    while w < e - M {
                        let dom: &str = if evening {
                            r.pick(&[("youtube.com", 4.0), ("reddit.com", 2.0 - drift), ("en.wikipedia.org", 1.0 + drift), ("x.com", 0.8), ("netflix.com", 0.7), ("khanacademy.org", 0.6 + drift)])
                        } else {
                            r.pick(&[("github.com", 4.0), ("stackoverflow.com", 2.0), ("docs.rs", 2.0), ("mail.google.com", 1.0), ("news.ycombinator.com", 0.8), ("youtube.com", 0.5), ("claude.ai", 1.2)])
                        };
                        let we = (w + r.range(M, 12 * M)).min(e);
                        put("web_sessions", None, json!({"domain": dom, "browser": "firefox", "device": pc, "start": w, "end": we, "tz_offset": tz}));
                        w = we + r.range(0, 20_000);
                    }
                }
                // idle gaps and short breaks
                t = e + if r.f() < 0.25 { r.range(5 * M, 30 * M) } else { r.range(1000, 30_000) };
            }
        }

        // ---- Windows gaming PC: some evenings and weekend afternoons
        if (weekend && r.f() < 0.7) || (!weekend && r.f() < 0.25) {
            let mut t = at(if weekend { 15.0 + r.f() * 2.0 } else { 19.0 + r.f() });
            let end = t + r.range(60 * M, 200 * M);
            while t < end && t < now - M {
                let app: &str = r.pick(&[("steam.exe", 3.0), ("discord.exe", 1.5), ("chrome.exe", 1.5), ("spotify.exe", 0.5)]);
                let e = (t + r.range(5 * M, 60 * M)).min(end).min(now);
                put("app_sessions", None, json!({"app": app.trim_end_matches(".exe"), "platform": "windows", "device": win, "start": t, "end": e, "tz_offset": tz}));
                if app == "chrome.exe" {
                    let dom: &str = r.pick(&[("youtube.com", 3.0), ("twitch.tv", 2.0), ("reddit.com", 1.0)]);
                    put("web_sessions", None, json!({"domain": dom, "browser": "chrome", "device": win, "start": t, "end": e, "tz_offset": tz}));
                }
                t = e + r.range(1000, 60_000);
            }
        }

        // ---- Phone: pickups across the waking day
        let wake = if weekend { 8.5 + r.f() * 1.5 } else { 6.8 + r.f() * 0.6 };
        let sleep = 23.0 + r.f() * 1.2;
        let mut h = wake;
        let mut hours: std::collections::BTreeMap<i64, (i64, i64, std::collections::BTreeMap<String, i64>)> = Default::default();
        while h < sleep {
            let t0 = at(h);
            if t0 > now {
                break;
            }
            let hour = t0.div_euclid(3_600_000) * 3_600_000;
            let ent = hours.entry(hour).or_default();
            ent.1 += 1; // pickup
            let unlocked = r.f() < 0.8;
            if unlocked {
                ent.0 += 1;
                let mut t = t0 + r.range(1000, 5000);
                let n_apps = r.range(1, 4);
                for _ in 0..n_apps {
                    let pkg: &str = r.pick(&[
                        ("com.whatsapp", 5.0), ("com.instagram.android", 3.0 * (1.2 - drift)), ("com.google.android.youtube", 2.0),
                        ("com.android.chrome", 2.0), ("com.google.android.gm", 1.0), ("com.duolingo", 0.6 + drift), ("com.ichi2.anki", 0.4 + drift),
                        ("com.spotify.music", 0.6), ("org.telegram.messenger", 1.0), ("com.google.android.apps.maps", 0.3),
                        ("com.selfkit.agenda", 0.8), ("com.reddit.frontpage", if back >= 18 { 1.0 } else { 0.0 }), ("com.google.android.calendar", 0.4),
                        ("com.zhiliaoapp.musically", if (30..40).contains(&back) { 2.5 } else { 0.0 }),
                    ]);
                    let len = match pkg {
                        "com.google.android.youtube" => r.range(2 * M, 25 * M),
                        "com.instagram.android" | "com.reddit.frontpage" | "com.zhiliaoapp.musically" => r.range(M, 15 * M),
                        "com.duolingo" | "com.ichi2.anki" => r.range(5 * M, 15 * M),
                        _ => r.range(10_000, 5 * M),
                    };
                    let e = (t + len).min(now);
                    if e - t < 1000 {
                        break;
                    }
                    let id = format!("a-seed-{t}-{pkg}");
                    put("app_sessions", Some(&id), json!({"app": label(pkg), "package": pkg, "platform": "android", "device": phone, "start": t, "end": e, "tz_offset": tz}));
                    t = e + r.range(500, 4000);
                }
            }
            // notifications this stretch
            for _ in 0..r.range(0, 4) {
                let pkg: &str = r.pick(&[("com.whatsapp", 5.0), ("com.google.android.gm", 2.0), ("org.telegram.messenger", 1.5), ("com.instagram.android", 1.5), ("com.google.android.calendar", 0.5)]);
                *ent.2.entry(pkg.to_string()).or_default() += 1;
            }
            h += (r.range(10, 55) as f64) / 60.0 * if (9.0..17.0).contains(&h) && !weekend { 1.5 } else { 1.0 };
        }
        for (hour, (unl, pick, notif)) in hours {
            let dt = chrono::DateTime::from_timestamp_millis(hour).unwrap();
            let id = format!("e-seed-{}", dt.format("%Y%m%d%H"));
            put("android_events", Some(&id), json!({"hour": hour, "tz_offset": tz, "device": phone, "unlocks": unl, "pickups": pick, "notifications": notif}));
        }
    }
    // app lifecycle on the phone: TikTok tried for 10 days, Reddit removed, Duolingo reinstalled
    let day_ms = 86_400_000i64;
    let ago = |d: i64, h: i64| now - d * day_ms + h * 3_600_000;
    let short_dev: String = phone.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().chars().rev().take(12).collect();
    for (pkg, l) in labels {
        let (installed, first, gone): (bool, i64, Option<i64>) = match pkg {
            "com.zhiliaoapp.musically" => (false, ago(40, 0), Some(ago(30, 2))),
            "com.reddit.frontpage" => (false, ago(400, 0), Some(ago(18, 1))),
            "com.duolingo" => (true, ago(9, -3), None),
            "com.selfkit.agenda" => (true, ago(60, 0), None),
            _ => (true, ago(400, 0), None),
        };
        let mut st = json!({"package": pkg, "label": l, "device": phone, "installed": installed, "first_install": first, "last_update": first, "system": false});
        if let Some(g) = gone {
            st["uninstalled_at"] = g.into();
        }
        put("app_installs", Some(&format!("i-{short_dev}-{pkg}")), st);
    }
    let ev = [
        ("com.selfkit.agenda", "Agenda", "installed", ago(60, 0)),
        ("com.zhiliaoapp.musically", "TikTok", "installed", ago(40, 0)),
        ("com.zhiliaoapp.musically", "TikTok", "uninstalled", ago(30, 2)),
        ("com.reddit.frontpage", "Reddit", "uninstalled", ago(18, 1)),
        ("com.duolingo", "Duolingo", "uninstalled", ago(12, 4)),
        ("com.duolingo", "Duolingo", "reinstalled", ago(9, -3)),
        ("com.whatsapp", "WhatsApp", "updated", ago(2, 5)),
    ];
    for (pkg, l, kind, at) in ev {
        put("app_changes", Some(&format!("c-seed-{pkg}-{kind}-{at}")), json!({"package": pkg, "label": l, "kind": kind, "at": at, "tz_offset": tz, "device": phone}));
    }
    c.execute_batch("COMMIT")?;
    // daily notes, some with photos (document page + landscape)
    let p1 = chrono_lib::notes::save_photo(&s, include_bytes!("seed-photo-2.webp"), "image/webp", 1200, 1600)?;
    let p2 = chrono_lib::notes::save_photo(&s, include_bytes!("seed-photo-1.webp"), "image/webp", 1600, 1200)?;
    let notes = [
        (1, "Finished the chapter on graphs and sent the draft to the group. Long walk by the river in the evening — no phone for two hours.", vec![p1.clone(), p2.clone()]),
        (3, "Too much YouTube after dinner. Phone in the other room tomorrow evening.", vec![]),
        (6, "Exam prep all day. Anki streak at 40 days.", vec![p1.clone()]),
        (12, "Lazy Sunday, visited grandma.", vec![p2.clone()]),
    ];
    for (back, text, photos) in notes {
        let d = (today - Duration::days(back)).format("%Y-%m-%d").to_string();
        chrono_lib::notes::save(&s, &d, text, &photos)?;
        n.set(n.get() + 1);
    }
    let _ = Local::now().hour();
    println!("seeded {} records into {}", n.get(), selfkit_core::paths::app_db("chrono").display());
    Ok(())
}
