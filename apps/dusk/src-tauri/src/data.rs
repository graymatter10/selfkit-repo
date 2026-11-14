//! Dusk's own records: `sleep_log` (corrections, ratings, naps),
//! `dusk_settings` (goals), kv `dusk.device` (per-device prefs) and the
//! local `dusk_nights` cache that Reflect reads.

use chrono::{Duration, NaiveDate};
use selfkit_core::store::Migration;
use selfkit_core::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::model::{self, Goals, LogEntry, Ms, NightView};

/// Error with a message fit for the user ("Wake time must be after bedtime").
#[derive(Debug)]
pub struct Error(pub String);

impl Error {
    #[allow(non_snake_case)]
    pub fn Invalid(s: String) -> Self {
        Self(s)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl From<selfkit_core::Error> for Error {
    fn from(e: selfkit_core::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self(e.to_string())
    }
}
pub type Result<T> = std::result::Result<T, Error>;

pub const MIGRATIONS: &[Migration] = &[Migration {
    name: "dusk/001_nights",
    sql: "CREATE TABLE IF NOT EXISTS dusk_nights (
            night TEXT PRIMARY KEY, bed INTEGER, wake INTEGER, tz_offset INTEGER NOT NULL,
            asleep_min INTEGER NOT NULL, nap_min INTEGER NOT NULL, quality INTEGER,
            source TEXT NOT NULL, late_use_min INTEGER NOT NULL, updated_at INTEGER NOT NULL);",
}];

pub const LOG: &str = "sleep_log";
pub const SETTINGS: &str = "dusk_settings";
pub const SETTINGS_ID: &str = "settings";
pub const DEVICE_KV: &str = "dusk.device";

pub fn main_id(night: &str) -> String {
    format!("n-{night}")
}

pub fn logs(store: &Store) -> Result<Vec<LogEntry>> {
    let mut st = store.conn().prepare("SELECT id, data FROM records WHERE collection=?1 AND deleted=0")?;
    let rows = st.query_map([LOG], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for (id, data) in rows.flatten() {
        if let Ok(mut l) = serde_json::from_str::<LogEntry>(&data) {
            l.id = id;
            out.push(l);
        }
    }
    Ok(out)
}

pub fn get_log(store: &Store, id: &str) -> Result<Option<LogEntry>> {
    Ok(store.get(id)?.filter(|r| !r.deleted).and_then(|r| {
        let mut l: LogEntry = serde_json::from_value(r.data).ok()?;
        l.id = r.id;
        Some(l)
    }))
}

fn to_value(l: &LogEntry) -> Value {
    let mut v = serde_json::to_value(l).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        o.remove("id");
    }
    v
}

/// Change the main entry of a night. `patch` keys: `bed`, `wake` ("HH:MM" or
/// null = use the estimate), `quality` (1–5 or null), `note`. Missing keys
/// stay as they are. Times are placed on the night with offset `tz`.
pub fn set_main(store: &Store, night: &str, patch: &Map<String, Value>, tz: i32) -> Result<LogEntry> {
    let n = model::parse_date(night).ok_or_else(|| Error::Invalid(format!("bad night {night}")))?;
    let id = main_id(night);
    let mut l = get_log(store, &id)?.unwrap_or_else(|| LogEntry {
        id: id.clone(),
        night: night.into(),
        kind: "main".into(),
        tz_offset: tz,
        created_at: selfkit_core::time::now_ms(),
        ..Default::default()
    });
    let time = |k: &str| -> Result<Option<Option<Ms>>> {
        match patch.get(k) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(s)) if s.trim().is_empty() => Ok(Some(None)),
            Some(Value::String(s)) => {
                model::time_in_night(n, s, tz).map(|t| Some(Some(t))).ok_or_else(|| Error::Invalid(format!("Can't read the time “{s}”")))
            }
            Some(Value::Number(x)) => Ok(Some(x.as_i64())),
            Some(_) => Err(Error::Invalid(format!("bad {k}"))),
        }
    };
    let (bed, wake) = (time("bed")?, time("wake")?);
    if let Some(b) = bed {
        l.bed = b;
    }
    if let Some(w) = wake {
        l.wake = w;
    }
    if bed.is_some() || wake.is_some() {
        l.tz_offset = tz;
    }
    if let Some(q) = patch.get("quality") {
        l.quality = match q.as_u64() {
            Some(x @ 1..=5) => Some(x as u8),
            None if q.is_null() => None,
            _ => return Err(Error::Invalid("quality must be 1–5".into())),
        };
    }
    let note = |v: &Value| v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(|s| s.chars().take(500).collect::<String>());
    if let Some(v) = patch.get("note_before") {
        l.note_before = note(v);
    }
    if let Some(v) = patch.get("note_after") {
        l.note_after = note(v);
    }
    if let (Some(b), Some(w)) = (l.bed, l.wake) {
        if w <= b {
            return Err(Error::Invalid("Wake time must be after bedtime".into()));
        }
        if w - b > 20 * model::HOUR {
            return Err(Error::Invalid("That's over 20 hours of sleep. Check the times.".into()));
        }
    }
    store.put(LOG, Some(&id), &to_value(&l))?;
    Ok(l)
}

/// Undo helper: put a night's main entry back exactly as it was (None = delete).
pub fn replace_main(store: &Store, night: &str, before: Option<&LogEntry>) -> Result<()> {
    let id = main_id(night);
    match before {
        Some(l) => {
            store.put(LOG, Some(&id), &to_value(l))?;
        }
        None => {
            if get_log(store, &id)?.is_some() {
                store.delete(&id)?;
            }
        }
    }
    Ok(())
}

/// Add a nap on day `night`, "HH:MM" start/end local with offset `tz`.
pub fn add_nap(store: &Store, night: &str, start: &str, end: &str, tz: i32) -> Result<LogEntry> {
    let n: NaiveDate = model::parse_date(night).ok_or_else(|| Error::Invalid(format!("bad night {night}")))?;
    let p = |s: &str| model::parse_hhmm(s).ok_or_else(|| Error::Invalid(format!("Can't read the time “{s}”")));
    let (a, b) = (p(start)?, p(end)?);
    let s = model::local_to_utc(n, a, tz);
    let mut e = model::local_to_utc(n, b, tz);
    if e <= s {
        e += Duration::days(1).num_milliseconds();
    }
    if e - s > 6 * model::HOUR {
        return Err(Error::Invalid("A nap can be 6 hours at most. Log a long sleep as the night instead.".into()));
    }
    let l = LogEntry {
        id: String::new(),
        night: night.into(),
        kind: "nap".into(),
        bed: Some(s),
        wake: Some(e),
        tz_offset: tz,
        created_at: selfkit_core::time::now_ms(),
        ..Default::default()
    };
    let r = store.put(LOG, None, &to_value(&l))?;
    Ok(LogEntry { id: r.id, ..l })
}

// ---- goals and prefs ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GoalsRec {
    pub goal_min: i64,
    /// Local "HH:MM".
    pub bedtime: String,
    pub winddown_min: i64,
}

impl Default for GoalsRec {
    fn default() -> Self {
        Self { goal_min: 480, bedtime: "23:00".into(), winddown_min: 30 }
    }
}

impl GoalsRec {
    pub fn model(&self) -> Goals {
        let t = model::parse_hhmm(&self.bedtime).unwrap_or_else(|| chrono::NaiveTime::from_hms_opt(23, 0, 0).unwrap());
        use chrono::Timelike;
        Goals { goal_min: self.goal_min, bedtime_min: (t.hour() * 60 + t.minute()) as i64, winddown_min: self.winddown_min }
    }
}

pub fn goals(store: &Store) -> Result<GoalsRec> {
    Ok(store.get(SETTINGS_ID)?.filter(|r| !r.deleted).and_then(|r| serde_json::from_value(r.data).ok()).unwrap_or_default())
}

pub fn save_goals(store: &Store, g: &GoalsRec) -> Result<GoalsRec> {
    let t = model::parse_hhmm(&g.bedtime).ok_or_else(|| Error::Invalid(format!("Can't read the time “{}”", g.bedtime)))?;
    if !(180..=840).contains(&g.goal_min) {
        return Err(Error::Invalid("Sleep goal must be between 3 and 14 hours".into()));
    }
    let g = GoalsRec { bedtime: t.format("%H:%M").to_string(), winddown_min: g.winddown_min.clamp(5, 180), ..g.clone() };
    store.put(SETTINGS, Some(SETTINGS_ID), &serde_json::to_value(&g)?)?;
    Ok(g)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DevicePrefs {
    pub winddown_on: bool,
    /// "system" | "light" | "dark"
    pub theme: String,
    /// Android: events imported up to here (UTC ms).
    pub import_cursor: Ms,
}

impl Default for DevicePrefs {
    fn default() -> Self {
        Self { winddown_on: false, theme: "system".into(), import_cursor: 0 }
    }
}

pub fn device(store: &Store) -> Result<DevicePrefs> {
    Ok(store.kv_get(DEVICE_KV)?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

pub fn save_device(store: &Store, p: &DevicePrefs) -> Result<()> {
    Ok(store.kv_set(DEVICE_KV, &serde_json::to_string(p)?)?)
}

/// Rewrite the local per-night cache (read by Reflect).
pub fn cache_nights(store: &Store, ns: &[NightView]) -> Result<()> {
    let now = selfkit_core::time::now_ms();
    let mut st = store.conn().prepare_cached(
        "INSERT INTO dusk_nights (night, bed, wake, tz_offset, asleep_min, nap_min, quality, source, late_use_min, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(night) DO UPDATE SET bed=excluded.bed, wake=excluded.wake, tz_offset=excluded.tz_offset,
           asleep_min=excluded.asleep_min, nap_min=excluded.nap_min, quality=excluded.quality, source=excluded.source,
           late_use_min=excluded.late_use_min, updated_at=excluded.updated_at",
    )?;
    for n in ns.iter().filter(|n| n.source != "ongoing") {
        st.execute((
            &n.night,
            n.bed,
            n.wake,
            n.tz_offset,
            n.asleep_min,
            n.nap_min,
            n.quality.map(i64::from),
            &n.source,
            n.late_use_min,
            now,
        ))?;
    }
    Ok(())
}

/// For the seed example and tests: build a nap/main JSON quickly.
pub fn log_json(night: &str, kind: &str, bed: Option<Ms>, wake: Option<Ms>, tz: i32, quality: Option<u8>) -> Value {
    json!({"night": night, "kind": kind, "bed": bed, "wake": wake, "tz_offset": tz, "quality": quality, "note_before": null, "note_after": null, "created_at": selfkit_core::time::now_ms()})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory("dusk", "d", MIGRATIONS).unwrap()
    }
    fn patch(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn main_entry_patch_and_undo() {
        let s = store();
        let l = set_main(&s, "2026-03-10", &patch(json!({"quality": 4})), 60).unwrap();
        assert_eq!((l.quality, l.bed, l.wake), (Some(4), None, None));
        let l = set_main(&s, "2026-03-10", &patch(json!({"bed": "23:30", "wake": "0715", "note_before": " Coffee late ", "note_after": ""})), 60).unwrap();
        assert_eq!((l.note_before.as_deref(), l.note_after.as_deref()), (Some("Coffee late"), None));
        assert_eq!(l.quality, Some(4));
        assert_eq!(l.wake.unwrap() - l.bed.unwrap(), (7 * 60 + 45) * model::MIN);
        assert!(set_main(&s, "2026-03-10", &patch(json!({"wake": "22:00"})), 60).is_err());
        assert!(set_main(&s, "2026-03-10", &patch(json!({"quality": 9})), 60).is_err());
        assert!(set_main(&s, "2026-03-10", &patch(json!({"bed": "zz"})), 60).is_err());
        let before = get_log(&s, &main_id("2026-03-10")).unwrap();
        set_main(&s, "2026-03-10", &patch(json!({"bed": null, "wake": null})), 60).unwrap();
        replace_main(&s, "2026-03-10", before.as_ref()).unwrap();
        assert_eq!(get_log(&s, &main_id("2026-03-10")).unwrap(), before);
        replace_main(&s, "2026-03-10", None).unwrap();
        assert!(logs(&s).unwrap().is_empty());
    }

    #[test]
    fn naps_goals_prefs_cache() {
        let s = store();
        let n = add_nap(&s, "2026-03-10", "14:00", "14:45", 0).unwrap();
        assert_eq!(n.wake.unwrap() - n.bed.unwrap(), 45 * model::MIN);
        assert!(add_nap(&s, "2026-03-10", "08:00", "17:00", 0).is_err());
        assert_eq!(logs(&s).unwrap().len(), 1);

        assert_eq!(goals(&s).unwrap(), GoalsRec::default());
        let g = save_goals(&s, &GoalsRec { goal_min: 450, bedtime: "2245".into(), winddown_min: 45 }).unwrap();
        assert_eq!(g.bedtime, "22:45");
        assert_eq!(goals(&s).unwrap().model().bedtime_min, 22 * 60 + 45);
        assert!(save_goals(&s, &GoalsRec { goal_min: 60, ..g.clone() }).is_err());

        let mut p = device(&s).unwrap();
        assert!(!p.winddown_on, "wind-down is off by default");
        p.winddown_on = true;
        save_device(&s, &p).unwrap();
        assert!(device(&s).unwrap().winddown_on);

        let logs = logs(&s).unwrap();
        let inp = model::Inputs { spans: &[], logs: &logs, external: &[], now: 0, now_active: false, fallback_tz: 0 };
        let v = model::night_view(&inp, model::parse_date("2026-03-10").unwrap());
        cache_nights(&s, &[v.clone(), v]).unwrap();
        let c: i64 = s.conn().query_row("SELECT nap_min FROM dusk_nights", [], |r| r.get(0)).unwrap();
        assert_eq!(c, 45);
    }
}
