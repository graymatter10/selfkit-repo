//! Shared Selfkit settings (day start, main currency, theme): one record in
//! `settings.db`, synced like any other record. See docs/design-system.md.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Result, Store};

pub const APP: &str = "settings";
pub const COLLECTION: &str = "settings";
/// Fixed id so every device converges on one settings record.
pub const SETTINGS_ID: &str = "00000000-0000-7000-8000-000000000001";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    CalmLight,
    SoftDark,
    PaperMono,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", content = "theme", rename_all = "snake_case")]
pub enum ThemeMode {
    System,
    Fixed(Theme),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeSetting {
    pub mode: ThemeMode,
    /// Used in System mode when the device is light (A or D).
    pub light: Theme,
    /// Used in System mode when the device is dark (B).
    pub dark: Theme,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// Minutes after local midnight when a "day" starts (default 0 = 00:00).
    pub day_start_min: u16,
    pub main_currency: String,
    pub theme: ThemeSetting,
    /// Global quick-capture shortcuts: action → accelerator (Tauri syntax,
    /// e.g. "Super+Shift+E"). Each opens a small popup that does the task and
    /// closes after saving. User-changeable in Settings; desktop only.
    pub shortcuts: BTreeMap<String, String>,
}

/// Per-field change clocks inside a record: `"_clock": {field: UTC ms of its last change}`.
/// Lets two devices that changed *different* fields merge without a conflict
/// (see `Store::field_merge`); the shared settings record uses it.
pub const CLOCK_FIELD: &str = "_clock";

/// Set `_clock[field] = now` for every top-level field that differs from `old`
/// (keeping the old clocks of unchanged fields).
pub fn stamp_fields(old: Option<&serde_json::Value>, new: &mut serde_json::Value) {
    let now = crate::time::now_ms();
    let empty = serde_json::Map::new();
    let old_obj = old.and_then(|v| v.as_object()).unwrap_or(&empty);
    let mut clock = old_obj.get(CLOCK_FIELD).and_then(|c| c.as_object()).cloned().unwrap_or_default();
    let Some(obj) = new.as_object_mut() else { return };
    for (k, v) in obj.iter() {
        if k == CLOCK_FIELD {
            continue;
        }
        if old_obj.get(k) != Some(v) || !clock.contains_key(k) {
            clock.insert(k.clone(), now.into());
        }
    }
    obj.insert(CLOCK_FIELD.into(), serde_json::Value::Object(clock));
}

/// Quick-capture actions that can have a global shortcut.
pub const SHORTCUT_ACTIONS: &[&str] = &["add_expense", "add_task", "rate_sleep", "toggle_tracking"];

pub fn default_shortcuts() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("add_expense".into(), "Super+Shift+E".into()),
        ("add_task".into(), "Super+Shift+T".into()),
        ("rate_sleep".into(), "Super+Shift+S".into()),
        ("toggle_tracking".into(), "Super+Shift+P".into()),
    ])
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            day_start_min: 0,
            main_currency: "USD".into(),
            theme: ThemeSetting { mode: ThemeMode::System, light: Theme::CalmLight, dark: Theme::SoftDark },
            shortcuts: default_shortcuts(),
        }
    }
}

impl Settings {
    pub fn load(store: &Store) -> Result<Self> {
        Ok(match store.get(SETTINGS_ID)? {
            Some(r) if !r.deleted => serde_json::from_value(r.data).unwrap_or_default(),
            _ => Self::default(),
        })
    }

    pub fn save(&self, store: &Store) -> Result<()> {
        let old = store.get(SETTINGS_ID)?.filter(|r| !r.deleted).map(|r| r.data);
        let mut data = serde_json::to_value(self)?;
        if let (Some(o), Some(old)) = (data.as_object_mut(), old.as_ref().and_then(|v| v.as_object())) {
            // Keep fields this version doesn't know.
            for (k, v) in old {
                o.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        stamp_fields(old.as_ref(), &mut data);
        store.put(COLLECTION, Some(SETTINGS_ID), &data)?;
        Ok(())
    }

    /// Change settings safely when several apps share `settings.db`: reload
    /// the stored record, apply `f`, and write back only the fields `f`
    /// changed (object fields such as `shortcuts` per key). Other fields keep
    /// their stored value, including ones this version doesn't know. Use this
    /// instead of `save` on a copy loaded earlier, which overwrites whatever
    /// another app changed in the meantime (e.g. the main currency).
    pub fn update(store: &Store, f: impl FnOnce(&mut Settings)) -> Result<Self> {
        let raw = store.get(SETTINGS_ID)?.filter(|r| !r.deleted && r.data.is_object()).map(|r| r.data);
        let before = raw.as_ref().and_then(|d| serde_json::from_value::<Settings>(d.clone()).ok()).unwrap_or_default();
        let mut after = before.clone();
        f(&mut after);
        let (b, a) = (serde_json::to_value(&before)?, serde_json::to_value(&after)?);
        let mut data = raw.unwrap_or_else(|| b.clone());
        if let (Some(out), Some(a), Some(b)) = (data.as_object_mut(), a.as_object(), b.as_object()) {
            for (k, v) in a {
                if b.get(k) == Some(v) {
                    continue;
                }
                match (out.get_mut(k).and_then(|x| x.as_object_mut()), v.as_object(), b.get(k).and_then(|x| x.as_object())) {
                    (Some(o), Some(nv), Some(ov)) => {
                        for (kk, vv) in nv {
                            if ov.get(kk) != Some(vv) {
                                o.insert(kk.clone(), vv.clone());
                            }
                        }
                        for kk in ov.keys().filter(|kk| !nv.contains_key(*kk)) {
                            o.remove(kk);
                        }
                    }
                    _ => {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        let old = store.get(SETTINGS_ID)?.filter(|r| !r.deleted).map(|r| r.data);
        stamp_fields(old.as_ref(), &mut data);
        store.put(COLLECTION, Some(SETTINGS_ID), &data)?;
        Self::load(store)
    }

    /// Change one shortcut. Rejects unknown actions and accelerators already
    /// used by another action. Empty string = disabled.
    pub fn set_shortcut(&mut self, action: &str, accel: &str) -> std::result::Result<(), String> {
        if !SHORTCUT_ACTIONS.contains(&action) {
            return Err(format!("unknown action {action}"));
        }
        let norm = |s: &str| s.to_ascii_lowercase().replace(' ', "");
        if !accel.is_empty()
            && let Some((other, _)) = self.shortcuts.iter().find(|(a, k)| a.as_str() != action && norm(k) == norm(accel))
        {
            return Err(format!("{accel} is already used by {other}"));
        }
        self.shortcuts.insert(action.into(), accel.into());
        Ok(())
    }

    /// Theme to show given whether the OS is currently dark.
    pub fn effective_theme(&self, os_dark: bool) -> Theme {
        match self.theme.mode {
            ThemeMode::Fixed(t) => t,
            ThemeMode::System if os_dark => self.theme.dark,
            ThemeMode::System => self.theme.light,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_roundtrip() {
        let s = Store::open_in_memory(APP, "d", &[]).unwrap();
        let mut st = Settings::load(&s).unwrap();
        assert_eq!(st.main_currency, "USD");
        assert_eq!(st.effective_theme(true), Theme::SoftDark);
        st.theme.mode = ThemeMode::Fixed(Theme::PaperMono);
        st.save(&s).unwrap();
        assert_eq!(Settings::load(&s).unwrap().effective_theme(true), Theme::PaperMono);
        let mut st = Settings::load(&s).unwrap();
        assert_eq!(st.shortcuts["add_task"], "Super+Shift+T");
        assert!(st.set_shortcut("add_task", "super+shift+e").is_err(), "clash with add_expense");
        st.set_shortcut("add_task", "Ctrl+Alt+T").unwrap();
        st.save(&s).unwrap();
        assert_eq!(Settings::load(&s).unwrap().shortcuts["add_task"], "Ctrl+Alt+T");
    }

    /// Two apps each hold a copy loaded at their start; each changes its own
    /// field. With `update`, neither undoes the other (with `save` on the
    /// stale copy, the second write put the main currency back to USD).
    #[test]
    fn update_keeps_other_apps_changes() {
        let s = Store::open_in_memory(APP, "d", &[]).unwrap();
        let stale_a = Settings::load(&s).unwrap();
        let stale_b = Settings::load(&s).unwrap();
        Settings::update(&s, |x| x.main_currency = "BDT".into()).unwrap();
        let mut bad = stale_b.clone();
        bad.set_shortcut("add_task", "Ctrl+Alt+T").unwrap();
        Settings::update(&s, |x| x.shortcuts.insert("add_task".into(), bad.shortcuts["add_task"].clone()).map(|_| ()).unwrap_or(())).unwrap();
        Settings::update(&s, |x| x.day_start_min = stale_a.day_start_min + 240).unwrap();
        let now = Settings::load(&s).unwrap();
        assert_eq!((now.main_currency.as_str(), now.day_start_min), ("BDT", 240));
        assert_eq!((now.shortcuts["add_task"].as_str(), now.shortcuts["add_expense"].as_str()), ("Ctrl+Alt+T", "Super+Shift+E"));
        // The old way: saving a stale whole copy loses the other app's change.
        bad.save(&s).unwrap();
        assert_eq!(Settings::load(&s).unwrap().main_currency, "USD");
        // Unknown fields (newer app version) survive an update.
        s.put(COLLECTION, Some(SETTINGS_ID), &serde_json::json!({"main_currency":"BDT","future":7})).unwrap();
        Settings::update(&s, |x| x.day_start_min = 60).unwrap();
        let d = s.get(SETTINGS_ID).unwrap().unwrap().data;
        assert_eq!((d["main_currency"].as_str(), d["future"].as_i64(), d["day_start_min"].as_i64()), (Some("BDT"), Some(7), Some(60)));
    }
}
