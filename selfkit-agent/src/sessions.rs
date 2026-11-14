//! Turns 1-second activity samples into `app_sessions` records in chrono.db.
//! A session is one continuous stretch of one app in the foreground while the
//! user is not idle. Open sessions are flushed every minute so a crash loses
//! at most ~60 s. See docs/apps/chrono/tracking.md.

use std::collections::HashSet;

use anyhow::Result;
use selfkit_core::{Store, time};
use serde_json::json;

use crate::tracker::Sample;

pub const COLLECTION: &str = "app_sessions";
/// A gap between samples larger than this means suspend/sleep: close the session.
const GAP_MS: i64 = 15_000;
const FLUSH_MS: i64 = 60_000;

#[derive(Debug, Clone)]
struct Open {
    id: String,
    app: String,
    start: i64,
    last: i64,
    flushed: i64,
}

pub struct Sessions {
    store: Store,
    platform: &'static str,
    pub idle_threshold_secs: u64,
    pub exclude: HashSet<String>,
    pub paused: bool,
    cur: Option<Open>,
}

impl Sessions {
    pub fn new(store: Store, platform: &'static str) -> Self {
        Self { store, platform, idle_threshold_secs: 120, exclude: HashSet::new(), paused: false, cur: None }
    }

    pub fn current_app(&self) -> Option<&str> {
        self.cur.as_ref().map(|o| o.app.as_str())
    }

    fn write(&self, o: &Open, end: i64) -> Result<()> {
        self.store.put(
            COLLECTION,
            Some(&o.id),
            &json!({
                "app": o.app, "platform": self.platform,
                "start": o.start, "end": end.max(o.start),
                "tz_offset": time::local_offset_min(),
            }),
        )?;
        Ok(())
    }

    fn close(&mut self, end: i64) -> Result<()> {
        if let Some(o) = self.cur.take() {
            // Drop sub-second blips; keep everything else.
            if end - o.start >= 1000 {
                self.write(&o, end)?;
            } else if o.flushed > 0 {
                self.store.delete(&o.id)?;
            }
        }
        Ok(())
    }

    /// Feed one sample taken at `now` (UTC ms).
    pub fn tick(&mut self, s: &Sample, now: i64) -> Result<()> {
        let idle = s.idle_secs >= self.idle_threshold_secs;
        let active: Option<String> = if self.paused || idle {
            None
        } else {
            s.app.clone().filter(|a| !self.exclude.contains(a))
        };

        if let Some(o) = &self.cur
            && now - o.last > GAP_MS {
                let end = o.last;
                self.close(end)?;
            }

        match (&mut self.cur, active) {
            (Some(o), Some(app)) if o.app == app => {
                o.last = now;
                if now - o.flushed >= FLUSH_MS {
                    o.flushed = now;
                    let o = o.clone();
                    self.write(&o, now)?;
                }
            }
            (_, active) => {
                // App switched, went idle, or paused. Back-date idle starts.
                let end = if idle { now - (s.idle_secs as i64) * 1000 } else { now };
                if let Some(o) = &self.cur {
                    let end = end.max(o.start).min(now);
                    self.close(end)?;
                }
                if let Some(app) = active {
                    self.cur = Some(Open { id: selfkit_core::store::new_id(), app, start: now, last: now, flushed: 0 });
                }
            }
        }
        Ok(())
    }

    /// Flush the open session (on shutdown).
    pub fn shutdown(&mut self, now: i64) -> Result<()> {
        let end = self.cur.as_ref().map(|o| o.last.max(now.min(o.last + GAP_MS))).unwrap_or(now);
        self.close(end)
    }
}

/// Seconds per app for sessions starting at or after `since` (UTC ms).
pub fn totals_since(store: &Store, since: i64) -> Result<Vec<(String, i64)>> {
    let mut st = store.conn().prepare(
        "SELECT json_extract(data,'$.app') AS app,
                SUM(json_extract(data,'$.end') - json_extract(data,'$.start')) / 1000 AS secs
         FROM records WHERE collection=?1 AND deleted=0 AND json_extract(data,'$.start') >= ?2
         GROUP BY app ORDER BY secs DESC",
    )?;
    let v = st.query_map((COLLECTION, since), |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?.collect::<Result<_, _>>()?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(app: Option<&str>, idle: u64) -> Sample {
        Sample { app: app.map(Into::into), idle_secs: idle }
    }

    fn sessions(store: &Store) -> Vec<(String, i64, i64)> {
        let mut v: Vec<_> = store
            .list(COLLECTION, 100, 0)
            .unwrap()
            .into_iter()
            .map(|r| (r.data["app"].as_str().unwrap().to_string(), r.data["start"].as_i64().unwrap(), r.data["end"].as_i64().unwrap()))
            .collect();
        v.sort_by_key(|x| x.1);
        v
    }

    #[test]
    fn switches_idle_gaps_and_exclusions() {
        let store = Store::open_in_memory("chrono", "d", &[]).unwrap();
        let mut t = Sessions::new(store, "linux");
        t.exclude.insert("keepassxc".into());
        let mut now = 1_000_000;
        for _ in 0..90 {
            t.tick(&s(Some("firefox"), 0), now).unwrap();
            now += 1000;
        }
        // switch to code for 30 s
        for _ in 0..30 {
            t.tick(&s(Some("code"), 0), now).unwrap();
            now += 1000;
        }
        // idle for 200 s while code still focused: back-dated end
        now += 199_000;
        t.tick(&s(Some("code"), 200), now).unwrap();
        // excluded app is not recorded
        for _ in 0..10 {
            now += 1000;
            t.tick(&s(Some("keepassxc"), 0), now).unwrap();
        }
        // terminal, then suspend gap
        for _ in 0..5 {
            now += 1000;
            t.tick(&s(Some("alacritty"), 0), now).unwrap();
        }
        let before_gap = now;
        now += 3_600_000;
        t.tick(&s(Some("alacritty"), 0), now).unwrap();
        now += 2000;
        t.shutdown(now).unwrap();

        let v = sessions(&t.store);
        let apps: Vec<_> = v.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(apps, ["firefox", "code", "alacritty", "alacritty"]);
        assert_eq!(v[0].2 - v[0].1, 90_000);
        // code: 30 s active, then idle back-dated to 200 s before the idle sample
        assert!((v[1].2 - v[1].1) <= 30_000, "code session {:?}", v[1]);
        assert_eq!(v[2].2, before_gap, "session closed at last sample before suspend");
        let tot = totals_since(&t.store, 0).unwrap();
        assert_eq!(tot[0].0, "firefox");
    }

    #[test]
    fn pause_stops_recording() {
        let store = Store::open_in_memory("chrono", "d", &[]).unwrap();
        let mut t = Sessions::new(store, "linux");
        t.paused = true;
        for i in 0..10 {
            t.tick(&s(Some("firefox"), 0), i * 1000).unwrap();
        }
        t.shutdown(10_000).unwrap();
        assert!(sessions(&t.store).is_empty());
    }
}
