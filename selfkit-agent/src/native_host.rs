//! Browser native-messaging host (`selfkit-agent native-host`, or started by the
//! browser with its own arguments). Protocol: each message is a u32 length in
//! native byte order followed by that many bytes of UTF-8 JSON, both ways.
//! Writes `web_sessions` into chrono.db. See docs/components/browser-extension.md.

use std::io::{Read, Write};

use anyhow::{Context, Result};
use selfkit_core::{Store, paths, time};
use serde_json::{Value, json};

use crate::chrono_settings::{ChronoSettings, normalize_domain};

pub const HOST_NAME: &str = "com.selfkit.agent";
pub const COLLECTION: &str = "web_sessions";
/// Incoming messages larger than this are rejected (the extension sends ≤ 500 sessions).
const MAX_IN: u32 = 1024 * 1024;
const DAY_MS: i64 = 86_400_000;
/// While paused, sessions that ended less than this long ago are dropped.
const PAUSE_LIVE_MS: i64 = 120_000;

/// True when the process was started by a browser: Chrome passes the caller
/// origin `chrome-extension://<id>/`; Firefox passes the manifest path and the
/// extension id.
pub fn started_by_browser(args: &[String]) -> bool {
    match args.get(1) {
        Some(a) if a.starts_with("chrome-extension://") => true,
        Some(a) if a.ends_with(".json") => args.get(2).is_some_and(|id| id.contains('@') || id.starts_with('{')),
        _ => false,
    }
}

pub fn read_msg(r: &mut impl Read) -> Result<Option<Value>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let n = u32::from_ne_bytes(len);
    anyhow::ensure!(n <= MAX_IN, "message too large ({n} bytes)");
    let mut buf = vec![0u8; n as usize];
    r.read_exact(&mut buf)?;
    Ok(Some(serde_json::from_slice(&buf).context("message is not JSON")?))
}

pub fn write_msg(w: &mut impl Write, v: &Value) -> Result<()> {
    let b = serde_json::to_vec(v)?;
    w.write_all(&(b.len() as u32).to_ne_bytes())?;
    w.write_all(&b)?;
    w.flush()?;
    Ok(())
}

#[derive(Debug, Default, PartialEq)]
pub struct WriteReport {
    pub written: u32,
    pub unchanged: u32,
    pub dropped: u32,
}

fn valid_id(id: &str) -> bool {
    (8..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Validate, filter and upsert sessions. Each session: `{id, domain, start, end, tz_offset}`.
pub fn write_sessions(
    store: &Store,
    cfg: &ChronoSettings,
    paused: bool,
    browser: &str,
    device: &str,
    sessions: &[Value],
    now: i64,
) -> Result<WriteReport> {
    let mut rep = WriteReport::default();
    let tx = store.conn().unchecked_transaction()?;
    for s in sessions {
        let id = s.get("id").and_then(Value::as_str).unwrap_or("");
        let domain = s.get("domain").and_then(Value::as_str).and_then(normalize_domain);
        let start = s.get("start").and_then(Value::as_i64).unwrap_or(0);
        let end = s.get("end").and_then(Value::as_i64).unwrap_or(0);
        let tz = s.get("tz_offset").and_then(Value::as_i64).unwrap_or(0).clamp(-840, 840);
        let Some(domain) = domain else {
            rep.dropped += 1;
            continue;
        };
        let sane = valid_id(id)
            && start >= 946_684_800_000
            && end - start >= 1000
            && end - start <= DAY_MS
            && end <= now + 300_000;
        if !sane || cfg.domain_excluded(&domain) || (paused && end >= now - PAUSE_LIVE_MS) {
            rep.dropped += 1;
            continue;
        }
        let data = json!({
            "domain": domain, "browser": browser, "device": device,
            "start": start, "end": end, "tz_offset": tz,
        });
        match store.get(id)? {
            Some(r) if r.collection != COLLECTION => {
                rep.dropped += 1;
                continue;
            }
            Some(r) if !r.deleted && r.data == data => {
                rep.unchanged += 1;
                continue;
            }
            // Deleted by the user in Chrono: don't resurrect.
            Some(r) if r.deleted => {
                rep.dropped += 1;
                continue;
            }
            _ => {}
        }
        store.put(COLLECTION, Some(id), &data)?;
        rep.written += 1;
    }
    tx.commit()?;
    Ok(rep)
}

/// Is a local agent running, and is it paused? (None = not reachable.)
fn agent_paused(rt: &tokio::runtime::Runtime) -> Option<bool> {
    rt.block_on(async {
        let r = tokio::time::timeout(std::time::Duration::from_millis(400), crate::ipc::request(&crate::ipc::Request::Status)).await;
        match r {
            Ok(Ok(v)) => Some(v["status"]["paused"].as_bool().unwrap_or(false)),
            _ => None,
        }
    })
}

fn browser_of(msg: &Value, args: &[String]) -> &'static str {
    match msg.get("browser").and_then(Value::as_str) {
        Some("firefox") => "firefox",
        Some(_) => "chrome",
        None if args.get(1).is_some_and(|a| a.starts_with("chrome-extension://")) => "chrome",
        None => "firefox",
    }
}

pub struct Host {
    store: Option<Store>,
    device: String,
    args: Vec<String>,
}

impl Host {
    pub fn new(args: Vec<String>) -> Result<Self> {
        Ok(Self { store: None, device: paths::device_id()?, args })
    }

    fn store(&mut self) -> Result<&Store> {
        if self.store.is_none() {
            self.store = Some(Store::open(paths::app_db("chrono"), "chrono", &self.device, &[])?);
        }
        Ok(self.store.as_ref().unwrap())
    }

    pub fn handle(&mut self, msg: &Value, agent: Option<bool>) -> Value {
        let now = time::now_ms();
        let res = (|| -> Result<Value> {
            let cfg = ChronoSettings::load(self.store()?);
            let chrono_paused = cfg.paused_at(now);
            let paused = chrono_paused || agent == Some(true);
            let mut reply = json!({
                "ok": true,
                "paused": paused,
                "paused_until": cfg.paused_until.filter(|_| chrono_paused),
                "exclude_domains": cfg.exclude_domains,
                "idle_minutes": cfg.idle_minutes,
                "agent_running": agent.is_some(),
                "device": self.device,
            });
            match msg.get("type").and_then(Value::as_str) {
                Some("sessions") => {
                    let browser = browser_of(msg, &self.args);
                    let sessions = msg.get("sessions").and_then(Value::as_array).cloned().unwrap_or_default();
                    let device = self.device.clone();
                    let rep = write_sessions(self.store()?, &cfg, paused, browser, &device, &sessions, now)?;
                    reply["written"] = json!(rep.written);
                    reply["unchanged"] = json!(rep.unchanged);
                    reply["dropped"] = json!(rep.dropped);
                }
                Some("status") | Some("hello") => {}
                other => anyhow::bail!("unknown message type {other:?}"),
            }
            Ok(reply)
        })();
        res.unwrap_or_else(|e| json!({"ok": false, "error": format!("{e:#}")}))
    }
}

/// Serve messages on stdin/stdout until the browser closes the pipe.
pub fn run(args: Vec<String>, rt: &tokio::runtime::Runtime) -> Result<()> {
    let mut host = Host::new(args)?;
    let mut input = std::io::stdin().lock();
    let mut out = std::io::stdout().lock();
    let mut agent: Option<Option<bool>> = None;
    loop {
        let msg = match read_msg(&mut input) {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(()),
            Err(e) => {
                let _ = write_msg(&mut out, &json!({"ok": false, "error": format!("{e:#}")}));
                return Err(e);
            }
        };
        let a = *agent.get_or_insert_with(|| agent_paused(rt));
        let reply = host.handle(&msg, a);
        if reply["ok"] == false {
            tracing::warn!("native host: {}", reply["error"]);
        }
        write_msg(&mut out, &reply)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_763_200_000_000;

    fn sess(id: &str, domain: &str, start: i64, end: i64) -> Value {
        json!({"id": id, "domain": domain, "start": start, "end": end, "tz_offset": 60})
    }

    #[test]
    fn framing_roundtrip() {
        let mut buf = Vec::new();
        write_msg(&mut buf, &json!({"type": "status"})).unwrap();
        write_msg(&mut buf, &json!({"a": [1, 2]})).unwrap();
        let mut r = buf.as_slice();
        assert_eq!(read_msg(&mut r).unwrap().unwrap()["type"], "status");
        assert_eq!(read_msg(&mut r).unwrap().unwrap()["a"][1], 2);
        assert!(read_msg(&mut r).unwrap().is_none());
        let mut big = (MAX_IN + 1).to_ne_bytes().to_vec();
        big.extend_from_slice(b"{}");
        assert!(read_msg(&mut big.as_slice()).is_err());
    }

    #[test]
    fn writes_filters_and_upserts() {
        let st = Store::open_in_memory("chrono", "dev1", &[]).unwrap();
        let cfg = ChronoSettings { exclude_domains: vec!["bank.example".into()], ..Default::default() };
        let id1 = "0199aaaa-0000-7000-8000-000000000001";
        let v = vec![
            sess(id1, "WWW.Example.com:443", NOW - 600_000, NOW - 300_000),
            sess("0199aaaa-0000-7000-8000-000000000002", "login.bank.example", NOW - 200_000, NOW - 100_000),
            sess("0199aaaa-0000-7000-8000-000000000003", "a.test/path", NOW - 200_000, NOW - 100_000),
            sess("0199aaaa-0000-7000-8000-000000000004", "b.test", NOW - 1000, NOW - 500), // < 1 s
            sess("0199aaaa-0000-7000-8000-000000000005", "b.test", NOW, NOW + 3_600_000), // future
            sess("bad id!", "b.test", NOW - 9000, NOW - 1000),
        ];
        let rep = write_sessions(&st, &cfg, false, "chrome", "dev1", &v, NOW).unwrap();
        assert_eq!(rep, WriteReport { written: 1, unchanged: 0, dropped: 5 });
        let r = st.get(id1).unwrap().unwrap();
        assert_eq!(r.collection, COLLECTION);
        assert_eq!(
            r.data,
            json!({"domain": "example.com", "browser": "chrome", "device": "dev1", "start": NOW - 600_000, "end": NOW - 300_000, "tz_offset": 60})
        );
        // same again: unchanged; extended end: rewritten
        let rep = write_sessions(&st, &cfg, false, "chrome", "dev1", &v[..1], NOW).unwrap();
        assert_eq!(rep.unchanged, 1);
        let rep = write_sessions(&st, &cfg, false, "chrome", "dev1", &[sess(id1, "example.com", NOW - 600_000, NOW - 1000)], NOW).unwrap();
        assert_eq!(rep.written, 1);
        assert_eq!(st.get(id1).unwrap().unwrap().data["end"], NOW - 1000);
        // deleted by the user: not resurrected
        st.delete(id1).unwrap();
        let rep = write_sessions(&st, &cfg, false, "chrome", "dev1", &[sess(id1, "example.com", NOW - 600_000, NOW - 500)], NOW).unwrap();
        assert_eq!(rep.dropped, 1);
    }

    #[test]
    fn pause_drops_live_sessions_only() {
        let st = Store::open_in_memory("chrono", "dev1", &[]).unwrap();
        let cfg = ChronoSettings::default();
        let v = vec![
            sess("0199aaaa-0000-7000-8000-00000000000a", "old.test", NOW - 900_000, NOW - 600_000),
            sess("0199aaaa-0000-7000-8000-00000000000b", "live.test", NOW - 60_000, NOW - 1000),
        ];
        let rep = write_sessions(&st, &cfg, true, "firefox", "dev1", &v, NOW).unwrap();
        assert_eq!((rep.written, rep.dropped), (1, 1));
    }

    #[test]
    fn host_handles_messages_and_settings() {
        let dir = tempfile::tempdir().unwrap();
        let st = Store::open(dir.path().join("chrono.db"), "chrono", "dev1", &[]).unwrap();
        st.put(crate::chrono_settings::COLLECTION, Some("settings"), &json!({"paused_until": time::now_ms() + 60_000, "exclude_domains": ["x.test"]})).unwrap();
        let mut h = Host { store: Some(st), device: "dev1".into(), args: vec!["agent".into(), "chrome-extension://abc/".into()] };
        let r = h.handle(&json!({"type": "status"}), None);
        assert_eq!(r["ok"], true);
        assert_eq!(r["paused"], true);
        assert_eq!(r["agent_running"], false);
        assert_eq!(r["exclude_domains"], json!(["x.test"]));
        let r = h.handle(&json!({"type": "nope"}), Some(false));
        assert_eq!(r["ok"], false);
    }

    #[test]
    fn detects_browser_launch() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(started_by_browser(&a(&["x", "chrome-extension://abcdef/", "--parent-window=0"])));
        assert!(started_by_browser(&a(&["x", "/home/u/.mozilla/native-messaging-hosts/com.selfkit.agent.json", "chrono@selfkit"])));
        assert!(!started_by_browser(&a(&["x", "run"])));
        assert!(!started_by_browser(&a(&["x"])));
    }
}
