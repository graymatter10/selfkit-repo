//! Goal/limit alerts: which notifications are due now. Each alert fires at
//! most once per local day per device (fired ids kept in local kv
//! `chrono.alerts`). Shown by the desktop timer or the Android worker.

use selfkit_core::{Result, Store};
use serde::{Deserialize, Serialize};

use crate::stats::{Engine, day_to_str};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Alert {
    pub id: String,
    pub title: String,
    pub body: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Fired {
    day: String,
    ids: Vec<String>,
}

const KEY: &str = "chrono.alerts";

fn target_label(g: &crate::settings::Goal) -> String {
    match g.scope.as_str() {
        "total" => "screen time".into(),
        "category" => {
            let mut c = g.target.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
        _ => g.target.clone(),
    }
}

pub fn fmt_min(ms: i64) -> String {
    let m = ms / 60_000;
    if m >= 60 { format!("{} h {:02} min", m / 60, m % 60) } else { format!("{m} min") }
}

/// Alerts due now that have not fired today; marks them fired.
pub fn due(e: &Engine, store: &Store, now: i64) -> Result<Vec<Alert>> {
    let want_goals = e.set.alerts && !e.set.is_paused(now) && e.set.goals.iter().any(|g| g.enabled);
    let today0 = e.today(now);
    let local_hour = (now + e.tz_now as i64 * 60_000).rem_euclid(crate::stats::DAY) / 3_600_000;
    let want_weekly = e.set.weekly_report && crate::stats::weekday(today0) == 0 && local_hour >= 8;
    if !want_goals && !want_weekly {
        return Ok(vec![]);
    }
    let today = e.today(now);
    let day = day_to_str(today);
    let mut fired: Fired = store.kv_get(KEY)?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    if fired.day != day {
        fired = Fired { day: day.clone(), ids: vec![] };
    }
    let mut out = Vec::new();
    if want_weekly && !fired.ids.iter().any(|i| i == "weekly") {
        let r = crate::stats::report(e, today - 7, now)?;
        if r.cur.screen > 0 {
            fired.ids.push("weekly".into());
            let avg = r.cur.screen / 7;
            let prev = r.prev.screen;
            let cmp = if prev == 0 {
                String::new()
            } else if r.cur.screen <= prev {
                format!(", {} less than the week before", fmt_min(prev - r.cur.screen))
            } else {
                format!(", {} more than the week before", fmt_min(r.cur.screen - prev))
            };
            out.push(Alert { id: "weekly".into(), title: "Your weekly report is ready".into(), body: format!("{} a day on average{cmp}.", fmt_min(avg)) });
        }
    }
    let a = e.days(today, today, now)?.remove(&today).unwrap_or_default();
    for gs in e.goals(&a).into_iter().filter(|_| want_goals) {
        let g = &gs.goal;
        let (id, title, body) = match gs.state.as_str() {
            "over" => (
                format!("{}:over", g.id),
                format!("Limit reached · {}", target_label(g)),
                format!("{} today, your limit is {}.", fmt_min(gs.used), fmt_min(g.minutes * 60_000)),
            ),
            "met" => (
                format!("{}:met", g.id),
                format!("Goal met · {}", target_label(g)),
                format!("{} today. Nicely done.", fmt_min(gs.used)),
            ),
            _ => continue,
        };
        if !fired.ids.contains(&id) {
            fired.ids.push(id.clone());
            out.push(Alert { id, title, body });
        }
    }
    if !out.is_empty() {
        store.kv_set(KEY, &serde_json::to_string(&fired)?)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{ChronoSettings, Goal};
    use serde_json::json;

    #[test]
    fn fires_once() {
        let s = Store::open_in_memory("chrono", "d", crate::stats::MIGRATIONS).unwrap();
        let t0 = 20_000 * crate::stats::DAY;
        s.put("app_sessions", None, &json!({"app":"YouTube","platform":"android","start": t0 + 3_600_000, "end": t0 + 2 * 3_600_000, "tz_offset": 0})).unwrap();
        let mut set = ChronoSettings::default();
        set.goals.push(Goal { id: "g1".into(), kind: "limit".into(), scope: "category".into(), target: "entertainment".into(), minutes: 30, enabled: true });
        let e = Engine::new(&s, set.clone(), 0, 0);
        let now = t0 + 3 * 3_600_000;
        let a = due(&e, &s, now).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].title, "Limit reached · Entertainment");
        assert!(due(&e, &s, now + 60_000).unwrap().is_empty());
        set.alerts = false;
        s.kv_set(KEY, "{}").unwrap();
        assert!(due(&Engine::new(&s, set, 0, 0), &s, now).unwrap().is_empty());
    }
}
