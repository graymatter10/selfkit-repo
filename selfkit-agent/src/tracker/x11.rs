//! i3/X11: active window from `_NET_ACTIVE_WINDOW` + `WM_CLASS`, idle time from
//! the MIT-SCREEN-SAVER extension. No window titles are read.

use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::screensaver::ConnectionExt as _;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;

use super::{Probe, Sample};

pub struct X11Probe {
    conn: RustConnection,
    root: Window,
    net_active: u32,
    net_pid: u32,
}

impl X11Probe {
    pub fn connect() -> Result<Self> {
        let (conn, screen) = x11rb::connect(None).context("connect to X11 (is DISPLAY set?)")?;
        let root = conn.setup().roots[screen].root;
        let net_active = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?.atom;
        let net_pid = conn.intern_atom(false, b"_NET_WM_PID")?.reply()?.atom;
        Ok(Self { conn, root, net_active, net_pid })
    }

    fn active_window(&self) -> Result<Option<Window>> {
        let r = self.conn.get_property(false, self.root, self.net_active, AtomEnum::WINDOW, 0, 1)?.reply()?;
        Ok(r.value32().and_then(|mut v| v.next()).filter(|w| *w != 0))
    }

    fn app_of(&self, w: Window) -> Result<Option<String>> {
        // WM_CLASS = "instance\0Class\0"; prefer Class.
        let r = self.conn.get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)?.reply()?;
        let parts: Vec<&[u8]> = r.value.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
        if let Some(c) = parts.get(1).or(parts.first()) {
            return Ok(Some(String::from_utf8_lossy(c).to_lowercase()));
        }
        // Fallback: process name from _NET_WM_PID.
        let r = self.conn.get_property(false, w, self.net_pid, AtomEnum::CARDINAL, 0, 1)?.reply()?;
        if let Some(pid) = r.value32().and_then(|mut v| v.next())
            && let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm")) {
                return Ok(Some(comm.trim().to_lowercase()));
            }
        Ok(None)
    }
}

impl Probe for X11Probe {
    fn sample(&mut self) -> Result<Sample> {
        let idle_ms = self.conn.screensaver_query_info(self.root)?.reply()?.ms_since_user_input;
        let app = match self.active_window()? {
            Some(w) => self.app_of(w).unwrap_or(None),
            None => None,
        };
        Ok(Sample { app, idle_secs: u64::from(idle_ms) / 1000 })
    }
}
