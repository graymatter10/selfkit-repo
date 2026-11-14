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
        store.put(COLLECTION, Some(SETTINGS_ID), &serde_json::to_value(self)?)?;
        Ok(())
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
}
