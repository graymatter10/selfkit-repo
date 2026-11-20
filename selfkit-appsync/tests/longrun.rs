//! Long-run "real world" simulation: years of daily use of all five apps on several
//! devices (PC agent, two phones, a tablet joining late, a phone replaced from the
//! recovery key, a device restored from a backup bundle), syncing over LAN and the
//! Postgres server at irregular times. An oracle (per-field last-writer-wins model of
//! what the user did) checks that nothing is lost or resurrected; every device must
//! converge. Findings, conflict counts, query times and DB sizes go to a report.
//!
//! Only runs when SELFKIT_LONGRUN_DAYS is set (CI job `longrun` in sync-e2e.yml):
//!   SELFKIT_LONGRUN_DAYS=30 SELFKIT_LONGRUN_SEED=1 SELFKIT_LONGRUN_OUT=/tmp/lr \
//!   cargo test -p selfkit-appsync --test longrun -- --ignored --nocapture
//! Needs SELFKIT_TEST_PG (Postgres). Optional: SELFKIT_LONGRUN_END (last simulated
//! day, YYYY-MM-DD; default 2026-11-20), SELFKIT_LONGRUN_STRICT=1 (fail on findings).
//! Output (SELFKIT_LONGRUN_OUT): devices/<name>/ data folders (open them with the real
//! apps via SELFKIT_DATA_DIR), report.md, findings.jsonl.
//!
//! Time: record clocks (`updated_at`, field clocks) are real time (the store has no
//! clock injection); the simulated date drives record content (dates, sessions,
//! nights). Clock skew is covered by dev/sync-e2e.sh (faketime).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use selfkit_appsync::secrets::{self, MemorySecrets, SecretStore};
use selfkit_appsync::{AppSync, AppSyncConfig, Choice, ServerConfig, SyncOutcome, SyncOwner, Transport, TransportMode, Trigger};
use selfkit_core::crypto::SecretKey;
use selfkit_core::lan::{LanConfig, LanState, StoreFactory};
use selfkit_core::store::Resolution;
use selfkit_core::Store;
use serde_json::{Map, Value, json};

const APPS: [&str; 5] = ["agenda", "hisab", "chrono", "dusk", "reflect"];
const STORES: [&str; 6] = ["agenda", "hisab", "chrono", "dusk", "reflect", "settings"];
const SETTINGS_ID: &str = selfkit_core::settings::SETTINGS_ID;
const HOME_TZ: i64 = 360;
/// Local minute each slot of the day starts at.
const SLOT_MIN: [i64; 7] = [7 * 60, 9 * 60, 11 * 60, 13 * 60, 16 * 60, 19 * 60, 22 * 60];
const PASS1: &str = "long run passphrase one";
const PASS2: &str = "long run passphrase number two";

// ---------------------------------------------------------------- rng, dates, clock

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
    /// Inclusive range.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo { lo } else { lo + self.below((hi - lo + 1) as u64) as i64 }
    }
    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < p
    }
    fn pick<T: Clone>(&mut self, v: &[T]) -> T {
        v[self.below(v.len() as u64) as usize].clone()
    }
    fn money(&mut self, lo: f64, hi: f64) -> f64 {
        let x = lo + (hi - lo) * ((self.next() >> 11) as f64 / (1u64 << 53) as f64);
        (x * 100.0).round() / 100.0
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
fn date(day: i64) -> String {
    let (y, m, d) = civil(day);
    format!("{y:04}-{m:02}-{d:02}")
}
fn parse_date(s: &str) -> Option<i64> {
    let p: Vec<i64> = s.get(..10)?.split('-').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    (p.len() == 3).then(|| days_from_civil(p[0], p[1], p[2]))
}
fn days_in_month(y: i64, m: i64) -> i64 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(ny, nm, 1) - days_from_civil(y, m, 1)
}
/// Month arithmetic from an anchor date, clamped to the month end (Jan 31 + 1 = Feb 28/29).
fn add_months(day: i64, n: i64) -> i64 {
    let (y, m, d) = civil(day);
    let t = y * 12 + (m - 1) + n;
    let (ny, nm) = (t.div_euclid(12), t.rem_euclid(12) + 1);
    days_from_civil(ny, nm, d.min(days_in_month(ny, nm)))
}
/// 0 = Sunday.
fn weekday(day: i64) -> i64 {
    (day + 4).rem_euclid(7)
}
fn sim_ms(day: i64, local_min: i64, tz: i64) -> i64 {
    (day * 1440 + local_min - tz) * 60_000
}
fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    format!("{:08x}", h as u32)
}

/// Every local write gets its own millisecond (field clocks are ms; exact ties are a
/// separate, rare case and would only add noise).
static LAST_MS: AtomicI64 = AtomicI64::new(0);
fn tick() {
    loop {
        if selfkit_core::time::now_ms() > LAST_MS.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_micros(250));
    }
}
fn after_op() {
    LAST_MS.fetch_max(selfkit_core::time::now_ms(), Ordering::Relaxed);
}

// ---------------------------------------------------------------- oracle

fn item_key(v: &Value) -> Option<String> {
    match v {
        Value::Object(o) => o.get("id").map(|i| format!("id:{i}")),
        Value::String(s) => Some(format!("s:{s}")),
        Value::Array(_) => Some(format!("j:{v}")),
        _ => None,
    }
}
/// Lists of id-objects / strings / tuples and objects are tracked entry by entry
/// (what the user changed), other values as a whole.
fn itemize(v: &Value) -> Option<(bool, BTreeMap<String, Value>)> {
    match v {
        Value::Array(a) => {
            let mut m = BTreeMap::new();
            for x in a {
                m.insert(item_key(x)?, x.clone());
            }
            Some((true, m))
        }
        Value::Object(o) => Some((false, o.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect())),
        _ => None,
    }
}
fn is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}
fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 160 { format!("{}…", s.chars().take(160).collect::<String>()) } else { s }
}

#[derive(Clone, Debug)]
enum OF {
    S(Value, i64),
    C { arr: bool, items: BTreeMap<String, (Option<Value>, i64)> },
}

#[derive(Clone, Debug)]
struct ORec {
    app: String,
    coll: String,
    del: (bool, i64),
    f: BTreeMap<String, OF>,
    legacy: bool,
    created: String,
}

#[derive(Default)]
struct Oracle {
    recs: HashMap<String, ORec>,
    by_coll: HashMap<(String, String), Vec<String>>,
}

impl Oracle {
    fn rec(&mut self, app: &str, coll: &str, id: &str, legacy: bool, today: &str) -> &mut ORec {
        if !self.recs.contains_key(id) {
            self.by_coll.entry((app.into(), coll.into())).or_default().push(id.into());
            self.recs.insert(
                id.into(),
                ORec { app: app.into(), coll: coll.into(), del: (false, 0), f: BTreeMap::new(), legacy, created: today.into() },
            );
        }
        self.recs.get_mut(id).unwrap()
    }

    fn set_scalar(r: &mut ORec, k: &str, v: Value, t: i64) {
        let cur_t = match r.f.get(k) {
            Some(OF::S(_, t0)) => *t0,
            Some(OF::C { items, .. }) => items.values().map(|x| x.1).max().unwrap_or(0),
            None => -1,
        };
        if t >= cur_t {
            r.f.insert(k.into(), OF::S(v, t));
        }
    }

    fn ensure_coll(r: &mut ORec, k: &str, arr: bool) -> &mut BTreeMap<String, (Option<Value>, i64)> {
        let conv = match r.f.get(k) {
            Some(OF::C { arr: a, .. }) if *a == arr => None,
            Some(OF::S(v, t)) => Some(match itemize(v) {
                Some((a, m)) if a == arr => m.into_iter().map(|(ik, iv)| (ik, (Some(iv), *t))).collect(),
                _ => BTreeMap::new(),
            }),
            _ => Some(BTreeMap::new()),
        };
        if let Some(items) = conv {
            r.f.insert(k.into(), OF::C { arr, items });
        }
        match r.f.get_mut(k) {
            Some(OF::C { items, .. }) => items,
            _ => unreachable!(),
        }
    }

    /// A field changed on a device from `old` (its local copy) to `new` at time `t`.
    /// `creation`: empty values of a new record don't count as edits.
    fn field(r: &mut ORec, k: &str, old: &Value, new: &Value, t: i64, creation: bool) {
        if !creation && old == new {
            return;
        }
        let tt = if creation && is_empty(new) { 0 } else { t };
        let oi = if old.is_null() { None } else { itemize(old) };
        match (oi, itemize(new)) {
            (Some((oa, om)), Some((na, nm))) if oa == na => {
                let items = Self::ensure_coll(r, k, na);
                for (ik, iv) in &nm {
                    if creation || om.get(ik) != Some(iv) {
                        let e = items.entry(ik.clone()).or_insert((None, -1));
                        if tt >= e.1 {
                            *e = (Some(iv.clone()), tt);
                        }
                    }
                }
                for ik in om.keys() {
                    if !nm.contains_key(ik) {
                        let e = items.entry(ik.clone()).or_insert((None, -1));
                        if t >= e.1 {
                            *e = (None, t);
                        }
                    }
                }
            }
            (None, Some((na, nm))) if old.is_null() => {
                let items = Self::ensure_coll(r, k, na);
                for (ik, iv) in nm {
                    let e = items.entry(ik).or_insert((None, -1));
                    if tt >= e.1 {
                        *e = (Some(iv), tt);
                    }
                }
            }
            _ => Self::set_scalar(r, k, new.clone(), tt),
        }
    }

    /// A user put: `old` = the device's local record before (None = new on this device).
    fn edit(&mut self, app: &str, coll: &str, id: &str, old: Option<&Value>, new: &Value, t: i64, legacy: bool, today: &str) {
        let r = self.rec(app, coll, id, legacy, today);
        let creation = old.is_none();
        let empty = Map::new();
        let om = old.and_then(Value::as_object).unwrap_or(&empty);
        let nm = new.as_object().unwrap_or(&empty);
        let keys: BTreeSet<&String> = om.keys().chain(nm.keys()).collect();
        for k in keys {
            let ov = om.get(k).cloned().unwrap_or(Value::Null);
            let nv = nm.get(k).cloned().unwrap_or(Value::Null);
            Self::field(r, k, &ov, &nv, t, creation);
        }
        if t >= r.del.1 || creation {
            r.del = (false, t.max(r.del.1));
        }
    }

    fn set_deleted(&mut self, id: &str, del: bool, t: i64) {
        if let Some(r) = self.recs.get_mut(id)
            && t >= r.del.1
        {
            r.del = (del, t);
        }
    }

    /// The user settled a conflict: what the device now holds is what they want, with
    /// the store's own field clocks (a newer edit elsewhere still wins).
    fn adopt(&mut self, rec: &selfkit_core::Record) {
        let Some(r) = self.recs.get_mut(&rec.id) else { return };
        let exp = Self::expected(r);
        let empty = Map::new();
        let em = exp.as_object().unwrap_or(&empty);
        let am = rec.data.as_object().unwrap_or(&empty);
        let keys: BTreeSet<String> = em.keys().chain(am.keys()).cloned().collect();
        for k in keys {
            let t = rec.fclock.get(&k).copied().unwrap_or(rec.updated_at);
            let ev = em.get(&k).cloned().unwrap_or(Value::Null);
            let av = am.get(&k).cloned().unwrap_or(Value::Null);
            if !same_field(r.f.get(&k), &av) {
                Self::field(r, &k, &ev, &av, t, false);
            }
        }
        if r.del.0 != rec.deleted {
            r.del = (rec.deleted, rec.updated_at.max(r.del.1));
        }
    }

    /// Replace the model of a record with what a device holds (after a finding, so one
    /// bug isn't reported again at every check).
    fn reset_to(&mut self, id: &str, data: &Value, deleted: bool) {
        let t = selfkit_core::time::now_ms();
        if let Some(r) = self.recs.get_mut(id) {
            r.f.clear();
            for (k, v) in data.as_object().cloned().unwrap_or_default() {
                Self::field(r, &k, &Value::Null, &v, t, false);
            }
            r.del = (deleted, t);
        }
    }

    fn expected(r: &ORec) -> Value {
        let mut m = Map::new();
        for (k, f) in &r.f {
            let v = match f {
                OF::S(v, _) => v.clone(),
                OF::C { arr: true, items } => Value::Array(items.values().filter_map(|x| x.0.clone()).collect()),
                OF::C { arr: false, items } => {
                    Value::Object(items.iter().filter_map(|(ik, x)| x.0.clone().map(|v| (ik.clone(), v))).collect())
                }
            };
            m.insert(k.clone(), v);
        }
        Value::Object(m)
    }

    fn pick(&self, rng: &mut Rng, app: &str, coll: &str) -> Option<String> {
        let v = self.by_coll.get(&(app.to_string(), coll.to_string()))?;
        if v.is_empty() {
            return None;
        }
        let n = v.len() as i64;
        let i = if rng.chance(0.85) { rng.range((n - 300).max(0), n - 1) } else { rng.range(0, n - 1) };
        Some(v[i as usize].clone())
    }
}

fn same_field(f: Option<&OF>, actual: &Value) -> bool {
    match f {
        None => actual.is_null(),
        Some(OF::S(v, _)) => v == actual || (v.is_null() && actual.is_null()),
        Some(OF::C { arr, items }) => {
            let live: BTreeMap<&String, &Value> = items.iter().filter_map(|(k, x)| x.0.as_ref().map(|v| (k, v))).collect();
            match itemize(actual) {
                Some((a, m)) if a == *arr => m.len() == live.len() && m.iter().all(|(k, v)| live.get(k) == Some(&v)),
                None if actual.is_null() => live.is_empty(),
                _ => false,
            }
        }
    }
}

/// What differs between the model and a device's record: (kind, field, expected, actual).
fn diff_record(r: &ORec, data: &Value, deleted: bool) -> Vec<(String, String, String, String)> {
    let mut out = vec![];
    if r.del.0 != deleted {
        out.push((
            if deleted { "record deleted (should be live)" } else { "record live (should be deleted)" }.to_string(),
            "_deleted".into(),
            r.del.0.to_string(),
            deleted.to_string(),
        ));
        return out;
    }
    let empty = Map::new();
    let am = data.as_object().unwrap_or(&empty);
    let keys: BTreeSet<&String> = r.f.keys().chain(am.keys()).collect();
    for k in keys {
        let av = am.get(k.as_str()).cloned().unwrap_or(Value::Null);
        let f = r.f.get(k.as_str());
        if same_field(f, &av) {
            continue;
        }
        let kind = match f {
            Some(OF::C { arr, items }) => match itemize(&av) {
                Some((a, m)) if a == *arr => {
                    let live: BTreeMap<&String, &Value> = items.iter().filter_map(|(k, x)| x.0.as_ref().map(|v| (k, v))).collect();
                    if m.keys().any(|k| !live.contains_key(k) && items.get(k).map(|x| x.0.is_none()).unwrap_or(false)) {
                        "removed list item/key came back"
                    } else if live.keys().any(|k| !m.contains_key(*k)) {
                        "list item/key lost"
                    } else if m.keys().any(|k| !items.contains_key(k)) {
                        "unknown list item/key"
                    } else {
                        "list item/key has an older value"
                    }
                }
                _ => "field shape changed",
            },
            Some(OF::S(v, _)) if is_empty(v) && !is_empty(&av) => "cleared field came back",
            Some(OF::S(v, _)) if !is_empty(v) && is_empty(&av) => "field edit lost (empty)",
            Some(OF::S(..)) => "field has an older/other value",
            None => "unknown field",
        };
        let ev = f.map(|f| Oracle::expected(&ORec { app: String::new(), coll: String::new(), del: (false, 0), f: BTreeMap::from([(k.to_string(), f.clone())]), legacy: false, created: String::new() })[k.as_str()].clone()).unwrap_or(Value::Null);
        out.push((kind.to_string(), k.to_string(), short(&ev), short(&av)));
    }
    out
}

// ---------------------------------------------------------------- devices

struct Device {
    name: String,
    dir: PathBuf,
    id: String,
    pc: bool,
    syncs: Vec<(String, AppSync)>,
    secrets: Arc<MemorySecrets>,
    stores: HashMap<String, Store>,
    active: bool,
    new_build: bool,
    lan_on: Option<bool>,
    hisab_opened: i64,
}

impl Device {
    fn st(&self, s: &str) -> &Store {
        &self.stores[s]
    }
    fn sync_of(&self, store: &str) -> &AppSync {
        self.syncs.iter().find(|(a, _)| a == store).map(|x| &x.1).unwrap_or(&self.syncs[0].1)
    }
}

fn b64(k: &SecretKey) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(k.as_bytes())
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn open_stores(dir: &Path, id: &str) -> HashMap<String, Store> {
    STORES.iter().map(|a| (a.to_string(), Store::open(dir.join(format!("{a}.db")), a, id, &[]).unwrap())).collect()
}

fn new_phone(root: &Path, name: &str, srv: &ServerConfig, pc_addr: SocketAddr) -> Device {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let secrets = Arc::new(MemorySecrets::default());
    let mut syncs = vec![];
    for app in APPS {
        let mut cfg = AppSyncConfig::new(app, &dir, secrets.clone());
        cfg.device_name = name.into();
        cfg.default_owner = SyncOwner::App;
        cfg.test_key = false;
        cfg.photos = None;
        syncs.push((app.to_string(), AppSync::new(cfg).unwrap()));
    }
    let s0 = &syncs[0].1;
    s0.set_server(Some(srv)).unwrap();
    let mut s = s0.settings();
    s.transport = TransportMode::Auto;
    s.lan_discovery_ms = 0;
    s.enabled = true;
    s.lan_peers = vec![pc_addr.to_string()];
    s0.set_settings(&s).unwrap();
    let id = s0.device_id().unwrap();
    let stores = open_stores(&dir, &id);
    Device { name: name.into(), dir, id, pc: false, syncs, secrets, stores, active: true, new_build: false, lan_on: None, hisab_opened: -1 }
}

async fn new_pc(root: &Path, key: &SecretKey) -> (Device, SocketAddr) {
    let dir = root.join("pc");
    std::fs::create_dir_all(&dir).unwrap();
    let id = format!("pc-{}", uuid::Uuid::now_v7());
    std::fs::write(dir.join("device_id"), &id).unwrap();
    let port = free_port();
    let p = dir.clone();
    let dev = id.clone();
    let factory: StoreFactory = Arc::new(move || {
        let mut m = HashMap::new();
        for app in STORES {
            m.insert(app.to_string(), Store::open(p.join(format!("{app}.db")), app, &dev, &[])?);
        }
        Ok(m)
    });
    let cfg = LanConfig {
        data_key: key.clone(),
        device_id: id.clone(),
        device_name: "Linux PC".into(),
        tcp_port: port,
        discovery_port: free_port(),
        static_peers: vec![],
        interval: Duration::from_secs(36_000),
    };
    tokio::spawn(selfkit_core::lan::serve(cfg, factory, Arc::new(LanState::default())));
    tokio::time::sleep(Duration::from_millis(200)).await;
    let stores = open_stores(&dir, &id);
    let d = Device {
        name: "pc".into(),
        dir,
        id,
        pc: true,
        syncs: vec![],
        secrets: Arc::new(MemorySecrets::default()),
        stores,
        active: true,
        new_build: false,
        lan_on: None,
        hisab_opened: -1,
    };
    (d, SocketAddr::from(([127, 0, 0, 1], port)))
}

async fn server() -> ServerConfig {
    let admin = std::env::var("SELFKIT_TEST_PG").expect("SELFKIT_TEST_PG");
    let (c, conn) = tokio_postgres::connect(&admin, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(conn);
    let db = format!("sk_longrun_{}", uuid::Uuid::now_v7().simple());
    c.batch_execute(&format!("CREATE DATABASE {db}")).await.unwrap();
    let base = admin.rsplit_once('/').unwrap().0;
    ServerConfig { pg_url: format!("{base}/{db}?sslmode=disable"), s3: None }
}

// ---------------------------------------------------------------- simulation

#[derive(serde::Serialize, Clone)]
struct Finding {
    day: i64,
    date: String,
    category: String,
    app: String,
    collection: String,
    id: String,
    field: String,
    expected: String,
    actual: String,
    note: String,
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

const PC: usize = 0;
const A: usize = 1;
const B: usize = 2;

#[derive(Clone)]
struct Ev {
    upgrade_a: i64,
    upgrade_pc: i64,
    upgrade_b: i64,
    pass_change: i64,
    outage: (i64, i64),
    b_offline: Vec<(i64, i64)>,
    c_join: i64,
    export: i64,
    d_restore: i64,
    stale_import: i64,
    b_lost: i64,
    day_start: (i64, i64),
    currency: i64,
    mini_trip: Option<(i64, i64)>,
}

struct Sim {
    rng: Rng,
    o: Oracle,
    devs: Vec<Device>,
    srv: ServerConfig,
    dead: ServerConfig,
    key: SecretKey,
    pc_addr: SocketAddr,
    root: PathBuf,
    out: PathBuf,
    start: i64,
    n: i64,
    di: i64,
    day: i64,
    tz: i64,
    ev: Ev,
    findings: Vec<Finding>,
    finding_keys: HashMap<String, u64>,
    conflicts: BTreeMap<String, u64>,
    seen_conflicts: HashSet<String>,
    passes: BTreeMap<String, u64>,
    pass_ms: BTreeMap<String, Vec<f64>>,
    q_ms: BTreeMap<(i64, String), Vec<f64>>,
    sizes: Vec<String>,
    recovery: String,
    passphrase: String,
    server_down: bool,
    pc_on: bool,
    away: bool,
    seq: u64,
    checks: u64,
}

fn names() -> (&'static [&'static str], &'static [&'static str]) {
    (
        &["Buy", "Call", "Fix", "Book", "Pay", "Email", "Clean", "Plan", "Renew", "Check", "Order", "Write", "Read", "Return"],
        &["groceries", "plumber", "passport", "electricity bill", "dentist", "car service", "rent", "report", "tickets", "gift", "laptop", "insurance", "garden", "taxes"],
    )
}

impl Sim {
    fn today(&self) -> String {
        date(self.day)
    }
    fn year(&self) -> i64 {
        self.di / 365 + 1
    }
    fn now_sim(&mut self, slot: usize) -> i64 {
        let m = SLOT_MIN[slot] + self.rng.range(0, 100);
        sim_ms(self.day, m, self.tz)
    }

    fn finding(&mut self, category: &str, app: &str, coll: &str, id: &str, field: &str, expected: String, actual: String, note: String) {
        let key = format!("{category} | {app}/{coll}.{field}");
        let n = {
            let e = self.finding_keys.entry(key.clone()).or_insert(0);
            *e += 1;
            *e
        };
        let f = Finding {
            day: self.di,
            date: self.today(),
            category: category.into(),
            app: app.into(),
            collection: coll.into(),
            id: id.into(),
            field: field.into(),
            expected,
            actual,
            note,
        };
        if n <= 5 {
            println!("LONGRUN FINDING day {} ({}) {key}: id {id} expected {} actual {} {}", f.day, f.date, f.expected, f.actual, f.note);
        }
        if let Ok(mut fh) = std::fs::OpenOptions::new().create(true).append(true).open(self.out.join("findings.jsonl")) {
            use std::io::Write;
            let _ = writeln!(fh, "{}", serde_json::to_string(&f).unwrap());
        }
        if n <= 20 {
            self.findings.push(f);
        }
    }

    // ---- user writes (each goes to the oracle) ----

    fn put(&mut self, i: usize, app: &str, coll: &str, id: Option<&str>, data: Value) -> String {
        tick();
        let d = &self.devs[i];
        let st = d.st(app);
        let old = id.and_then(|x| st.get(x).unwrap());
        let rec = st.put(coll, id, &data).unwrap();
        after_op();
        if !d.new_build {
            st.conn().execute("UPDATE records SET fclock='{}' WHERE id=?", [&rec.id]).unwrap();
        }
        let legacy = !d.new_build;
        let today = date(self.day);
        let was_deleted = old.as_ref().map(|r| r.deleted).unwrap_or(false);
        self.o.edit(app, coll, &rec.id, old.as_ref().map(|r| &r.data), &rec.data, rec.updated_at, legacy, &today);
        if was_deleted {
            self.o.set_deleted(&rec.id, false, rec.updated_at);
        }
        self.seq += 1;
        rec.id
    }
    fn del(&mut self, i: usize, app: &str, id: &str, deleted: bool) {
        tick();
        let st = self.devs[i].st(app);
        if deleted { st.delete(id).unwrap() } else { st.restore(id).unwrap() }
        after_op();
        let t = st.get(id).unwrap().unwrap().updated_at;
        self.o.set_deleted(id, deleted, t);
        self.seq += 1;
    }
    /// Live local copy (what the user sees on that device).
    fn local(&self, i: usize, app: &str, id: &str) -> Option<Value> {
        self.devs[i].st(app).get(id).unwrap().filter(|r| !r.deleted).map(|r| r.data)
    }
    fn pick_local(&mut self, i: usize, app: &str, coll: &str, ok: impl Fn(&Value) -> bool) -> Option<(String, Value)> {
        for _ in 0..10 {
            let id = self.o.pick(&mut self.rng, app, coll)?;
            if let Some(v) = self.local(i, app, &id)
                && ok(&v)
            {
                return Some((id, v));
            }
        }
        None
    }

    // ---- agenda ----

    fn agenda_new(&mut self, i: usize, slot: usize) {
        let (verbs, nouns) = names();
        let title = format!("{} {}", self.rng.pick(verbs), self.rng.pick(nouns));
        let notes = match self.rng.below(10) {
            0..=5 => String::new(),
            6..=8 => format!("note {}", self.rng.below(1000)),
            _ => (0..self.rng.range(2, 6)).map(|l| format!("- line {l}: details {}", self.rng.below(10_000))).collect::<Vec<_>>().join("\n"),
        };
        let steps: Vec<Value> = (0..if self.rng.chance(0.3) { self.rng.range(1, 4) } else { 0 })
            .map(|s| json!({"id": format!("s{}-{}", self.seq, s), "title": format!("step {s}"), "done": false}))
            .collect();
        let tags: Vec<Value> = ["home", "work", "errand", "health", "money"].iter().filter(|_| self.rng.chance(0.2)).map(|t| json!(t)).collect();
        let created = self.now_sim(slot);
        let mut d = json!({"title": title, "notes": notes, "severity": self.rng.range(1, 5), "list": self.rng.pick(&["inbox", "home", "work"]),
            "steps": steps, "tags": tags, "photos": [], "created_at": created});
        if self.rng.chance(0.7) {
            let dd = self.day + self.rng.range(0, 14);
            let start = if self.rng.chance(0.25) { date(dd) } else { format!("{}T{:02}:{:02}", date(dd), self.rng.range(7, 21), self.rng.pick(&[0, 15, 30, 45])) };
            d["start"] = json!(start);
            if self.rng.chance(0.4) {
                d["reminder_min"] = json!(self.rng.pick(&[0, 10, 30, 60]));
            }
            if self.rng.chance(0.1) {
                d["repeat"] = json!(self.rng.pick(&["daily", "weekdays", "weekly", "monthly", "yearly"]));
                // Month-end anchors for the date-math edge cases.
                if self.rng.chance(0.3) {
                    let (y, m, _) = civil(self.day);
                    d["start"] = json!(format!("{}T08:00", date(days_from_civil(y, m, days_in_month(y, m)))));
                }
            }
        }
        self.put(i, "agenda", "tasks", None, d);
    }

    fn advance(start: &str, rep: &str) -> String {
        let Some(day) = parse_date(start) else { return start.to_string() };
        let nd = match rep {
            "daily" => day + 1,
            "weekdays" => {
                let mut x = day + 1;
                while matches!(weekday(x), 0 | 6) {
                    x += 1;
                }
                x
            }
            "weekly" => day + 7,
            "monthly" => add_months(day, 1),
            _ => add_months(day, 12),
        };
        format!("{}{}", date(nd), start.get(10..).unwrap_or(""))
    }

    fn agenda_complete(&mut self, i: usize, slot: usize) {
        let Some((id, mut t)) = self.pick_local(i, "agenda", "tasks", |v| v.get("done_at").map(Value::is_null).unwrap_or(true)) else { return };
        let before = t.clone();
        let at = self.now_sim(slot);
        let mut c = json!({"task": id, "title": t["title"], "done_at": at, "tz_offset_min": self.tz, "tags": t["tags"]});
        if let Some(s) = t.get("start").and_then(Value::as_str) {
            c["due"] = json!(s);
            c["on_time"] = json!(parse_date(s).map(|d| d >= self.day).unwrap_or(true));
        }
        let cid = self.put(i, "agenda", "completions", None, c);
        if let Some(rep) = t.get("repeat").and_then(Value::as_str).map(str::to_string)
            && let Some(s) = t.get("start").and_then(Value::as_str).map(str::to_string)
        {
            t["start"] = json!(Self::advance(&s, &rep));
            t["last_done_at"] = json!(at);
            if let Some(st) = t.get_mut("steps").and_then(Value::as_array_mut) {
                for s in st {
                    s["done"] = json!(false);
                }
            }
        } else {
            t["done_at"] = json!(at);
            if let Some(o) = t.as_object_mut() {
                o.remove("stage");
            }
        }
        t["last_completion"] = json!(cid);
        self.put(i, "agenda", "tasks", Some(&id), t);
        if self.rng.chance(0.04) {
            // Undo: the completion goes, the task comes back as it was.
            self.del(i, "agenda", &cid, true);
            self.put(i, "agenda", "tasks", Some(&id), before);
        }
    }

    fn agenda_edit(&mut self, i: usize) {
        let Some((id, mut t)) = self.pick_local(i, "agenda", "tasks", |_| true) else { return };
        match self.rng.below(9) {
            0 => t["title"] = json!(format!("{} (v{})", t["title"].as_str().unwrap_or("").split(" (v").next().unwrap_or(""), self.seq)),
            1 => {
                let n = t["notes"].as_str().unwrap_or("").to_string();
                t["notes"] = json!(if n.is_empty() { format!("added {}", self.seq) } else { format!("{n}\n- more {}", self.seq) });
            }
            2 => t["severity"] = json!(self.rng.range(1, 5)),
            3 | 4 => {
                if let Some(st) = t.get_mut("steps").and_then(Value::as_array_mut)
                    && !st.is_empty()
                {
                    let k = self.rng.below(st.len() as u64) as usize;
                    let d = st[k]["done"].as_bool().unwrap_or(false);
                    st[k]["done"] = json!(!d);
                } else {
                    t["steps"] = json!([{"id": format!("s{}", self.seq), "title": "first step", "done": false}]);
                }
            }
            5 => {
                let s = json!({"id": format!("s{}", self.seq), "title": format!("step {}", self.seq), "done": false});
                match t.get_mut("steps").and_then(Value::as_array_mut) {
                    Some(st) => st.push(s),
                    None => t["steps"] = json!([s]),
                }
            }
            6 => {
                if let Some(st) = t.get_mut("steps").and_then(Value::as_array_mut)
                    && !st.is_empty()
                {
                    let k = self.rng.below(st.len() as u64) as usize;
                    st.remove(k);
                }
            }
            7 => {
                let tag = self.rng.pick(&["home", "work", "errand", "health", "money", "later"]);
                let tags = t.get_mut("tags").and_then(Value::as_array_mut);
                match tags {
                    Some(v) if v.contains(&json!(tag)) => v.retain(|x| x != &json!(tag)),
                    Some(v) => v.push(json!(tag)),
                    None => t["tags"] = json!([tag]),
                }
            }
            _ => {
                let dd = self.day + self.rng.range(0, 10);
                t["start"] = json!(format!("{}T{:02}:00", date(dd), self.rng.range(7, 21)));
                if self.rng.chance(0.3) {
                    t["stage"] = json!("doing");
                }
            }
        }
        self.put(i, "agenda", "tasks", Some(&id), t);
    }

    fn agenda_delete(&mut self, i: usize) {
        let Some((id, _)) = self.pick_local(i, "agenda", "tasks", |_| true) else { return };
        self.del(i, "agenda", &id, true);
        if self.rng.chance(0.3) {
            self.del(i, "agenda", &id, false);
        }
    }

    fn agenda_focus(&mut self, i: usize, slot: usize) {
        let s = self.now_sim(slot);
        let task = self.pick_local(i, "agenda", "tasks", |_| true);
        let min = self.rng.pick(&[25, 25, 50, 5]);
        let mut f = json!({"title": task.as_ref().map(|t| t.1["title"].clone()).unwrap_or(json!("Focus")), "kind": if min == 5 { "break" } else { "focus" },
            "start": s, "end": s + min * 60_000, "minutes": min, "planned_min": min, "completed": self.rng.chance(0.8), "tz_offset_min": self.tz});
        if let Some((tid, _)) = task {
            f["task"] = json!(tid);
        }
        self.put(i, "agenda", "focus", None, f);
    }

    // ---- hisab ----

    fn hisab_setup(&mut self, i: usize) {
        for (id, name, kind, cur, open) in [
            ("acc-cash", "Cash", "cash", "BDT", 5000.0),
            ("acc-bank", "Bank", "bank", "BDT", 120000.0),
            ("acc-card", "Card", "card", "USD", 0.0),
            ("acc-wallet", "Wallet", "wallet", "EUR", 50.0),
        ] {
            self.put(i, "hisab", "accounts", Some(id), json!({"name": name, "kind": kind, "currency": cur, "explicit_currency": true, "opening": open, "archived": false, "order": 0}));
        }
        for (id, name, kind, budget) in [
            ("cat-food", "Food", "expense", 15000.0),
            ("cat-transport", "Transport", "expense", 4000.0),
            ("cat-bills", "Bills", "expense", 9000.0),
            ("cat-fun", "Fun", "expense", 3000.0),
            ("cat-health", "Health", "expense", 0.0),
            ("cat-shop", "Shopping", "expense", 6000.0),
            ("cat-salary", "Salary", "income", 0.0),
            ("cat-other", "Other", "income", 0.0),
        ] {
            self.put(i, "hisab", "categories", Some(id), json!({"name": name, "kind": kind, "icon": "dot", "color": "grey", "budget": budget, "archived": false, "order": 0}));
        }
        self.put(i, "hisab", "rates", Some("hisab-rates"), json!({"base": "USD", "rates": {"BDT": 110.0, "EUR": 0.92, "INR": 83.0}, "manual": {}, "fetched_at": sim_ms(self.day, 400, self.tz)}));
        let (y, m, _) = civil(self.day);
        let month_end = days_from_civil(y, m, days_in_month(y, m));
        // A Feb 29 in range for the yearly item, else a date soon.
        let mut ins = self.day + 10;
        for d in self.day..self.day + self.n {
            let (_, mm, dd) = civil(d);
            if mm == 2 && dd == 29 {
                ins = d;
                break;
            }
        }
        for (id, name, freq, start, amount, cat, acc) in [
            ("rec-rent", "Rent", "monthly", month_end, 25000.0, "cat-bills", "acc-bank"),
            ("rec-net", "Internet", "monthly", self.day + 4, 1200.0, "cat-bills", "acc-bank"),
            ("rec-ins", "Insurance", "yearly", ins, 300.0, "cat-health", "acc-card"),
            ("rec-gym", "Gym", "weekly", self.day + 2, 900.0, "cat-health", "acc-cash"),
        ] {
            let cur = if acc == "acc-card" { "USD" } else { "BDT" };
            self.put(i, "hisab", "recurring", Some(id), json!({"name": name, "freq": freq, "interval": 1, "start": date(start), "next": date(start), "end": null,
                "active": true, "subscription": id == "rec-net", "template": {"kind": "expense", "amount": amount, "currency": cur, "account": acc, "category": cat, "note": name, "tags": []}}));
        }
        self.put(i, "hisab", "goals", Some("goal-trip"), json!({"name": "Trip", "target": 80000.0, "currency": "BDT", "deadline": date(self.day + 300), "contributions": [], "archived": false}));
    }

    fn occurrence(freq: &str, start: i64, n: i64) -> i64 {
        match freq {
            "daily" => start + n,
            "weekly" => start + 7 * n,
            "monthly" => add_months(start, n),
            _ => add_months(start, 12 * n),
        }
    }

    /// What the Hisab app does when opened (once a day per device): rates refresh and
    /// recurring generation, both from this device's local copy.
    fn hisab_open(&mut self, i: usize) {
        if self.devs[i].hisab_opened == self.day {
            return;
        }
        self.devs[i].hisab_opened = self.day;
        if let Some(r) = self.local(i, "hisab", "hisab-rates")
            && r["fetched_at"].as_i64().unwrap_or(0) < sim_ms(self.day, 0, self.tz)
            && !self.away_from_net(i)
        {
            let mut r = r;
            let jit = |x: f64, rng: &mut Rng| ((x * (1.0 + (rng.below(200) as f64 - 100.0) / 5000.0)) * 10000.0).round() / 10000.0;
            r["rates"] = json!({"BDT": jit(110.0, &mut self.rng), "EUR": jit(0.92, &mut self.rng), "INR": jit(83.0, &mut self.rng)});
            r["fetched_at"] = json!(sim_ms(self.day, SLOT_MIN[0], self.tz));
            self.put(i, "hisab", "hisab-rates", Some("hisab-rates"), r);
        }
        for rid in ["rec-rent", "rec-net", "rec-ins", "rec-gym"] {
            let Some(mut r) = self.local(i, "hisab", rid) else { continue };
            if !r["active"].as_bool().unwrap_or(false) {
                continue;
            }
            let freq = r["freq"].as_str().unwrap_or("monthly").to_string();
            let start = parse_date(r["start"].as_str().unwrap_or("")).unwrap_or(self.day);
            let mut next = parse_date(r["next"].as_str().unwrap_or("")).unwrap_or(start);
            let mut made = 0;
            while next <= self.day && made < 40 {
                let mut tx = r["template"].clone();
                tx["date"] = json!(date(next));
                tx["time"] = json!("09:00");
                tx["splits"] = json!([]);
                tx["photos"] = json!([]);
                tx["source"] = json!("recurring");
                tx["recurring"] = json!(rid);
                tx["created_at"] = json!(sim_ms(self.day, SLOT_MIN[0], self.tz));
                self.put(i, "hisab", "transactions", None, tx);
                made += 1;
                let mut k = 1;
                while Self::occurrence(&freq, start, k) <= next {
                    k += 1;
                }
                next = Self::occurrence(&freq, start, k);
            }
            if made > 0 {
                r["next"] = json!(date(next));
                self.put(i, "hisab", "recurring", Some(rid), r);
            }
        }
    }

    fn hisab_tx(&mut self, i: usize, slot: usize) {
        self.hisab_open(i);
        let acc = self.rng.pick(&["acc-cash", "acc-cash", "acc-bank", "acc-card", "acc-wallet"]);
        let cur = match acc {
            "acc-card" => "USD",
            "acc-wallet" => "EUR",
            _ => "BDT",
        };
        let amount = if cur == "BDT" { self.rng.money(20.0, 3000.0) } else { self.rng.money(1.0, 60.0) };
        let cat = self.rng.pick(&["cat-food", "cat-food", "cat-transport", "cat-fun", "cat-shop", "cat-health", "cat-bills"]);
        let late = self.rng.chance(0.1);
        let dd = if late { self.day - self.rng.range(1, 3) } else { self.day };
        let mut tx = json!({"kind": "expense", "amount": amount, "currency": cur, "date": date(dd), "time": format!("{:02}:{:02}", SLOT_MIN[slot] / 60, self.rng.range(0, 59)),
            "account": acc, "category": cat, "splits": [], "tags": [], "note": self.rng.pick(&["", "", "lunch", "bus", "groceries", "coffee", "medicine", "gift"]),
            "photos": [], "source": "manual", "created_at": self.now_sim(slot)});
        if self.rng.chance(0.05) {
            let a = (amount * 0.6 * 100.0).round() / 100.0;
            tx["splits"] = json!([{"category": cat, "amount": a}, {"category": "cat-shop", "amount": ((amount - a) * 100.0).round() / 100.0}]);
        }
        if self.rng.chance(0.05) {
            tx["kind"] = json!("transfer");
            tx["account"] = json!("acc-bank");
            tx["currency"] = json!("BDT");
            tx["to_account"] = json!("acc-cash");
            tx["to_amount"] = json!(amount);
            tx["category"] = json!("");
        }
        if self.rng.chance(0.03) {
            tx["currency"] = json!("INR");
        }
        if civil(self.day).2 == 1 && slot == 0 && self.rng.chance(0.8) {
            tx = json!({"kind": "income", "amount": 90000.0, "currency": "BDT", "date": date(self.day), "time": "08:00", "account": "acc-bank", "category": "cat-salary",
                "splits": [], "tags": ["salary"], "note": "Salary", "photos": [], "source": "manual", "created_at": self.now_sim(slot)});
        }
        self.put(i, "hisab", "transactions", None, tx);
    }

    fn hisab_edit(&mut self, i: usize) {
        self.hisab_open(i);
        let Some((id, mut t)) = self.pick_local(i, "hisab", "transactions", |_| true) else { return };
        match self.rng.below(5) {
            0 => t["amount"] = json!(self.rng.money(10.0, 2000.0)),
            1 => t["note"] = json!(format!("{} (fixed)", t["note"].as_str().unwrap_or(""))),
            2 => t["category"] = json!(self.rng.pick(&["cat-food", "cat-transport", "cat-fun", "cat-shop"])),
            3 => {
                let d = parse_date(t["date"].as_str().unwrap_or("")).unwrap_or(self.day);
                t["date"] = json!(date(d - 1));
            }
            _ => {
                let tag = self.rng.pick(&["trip", "family", "work", "tax"]);
                match t.get_mut("tags").and_then(Value::as_array_mut) {
                    Some(v) if v.contains(&json!(tag)) => v.retain(|x| x != &json!(tag)),
                    Some(v) => v.push(json!(tag)),
                    None => t["tags"] = json!([tag]),
                }
            }
        }
        self.put(i, "hisab", "transactions", Some(&id), t);
    }

    fn hisab_delete(&mut self, i: usize) {
        let Some((id, t)) = self.pick_local(i, "hisab", "transactions", |_| true) else { return };
        if t["source"] == "recurring" {
            return;
        }
        self.del(i, "hisab", &id, true);
        if self.rng.chance(0.35) {
            self.del(i, "hisab", &id, false);
        }
    }

    fn hisab_misc(&mut self, i: usize) {
        self.hisab_open(i);
        match self.rng.below(6) {
            0 => {
                let person = self.rng.pick(&["Karim", "Rina", "Sam", "Joy"]);
                self.put(i, "hisab", "loans", None, json!({"direction": self.rng.pick(&["lent", "borrowed"]), "person": person, "title": "Loan", "description": "",
                    "amount": self.rng.money(1000.0, 20000.0), "currency": "BDT", "given": self.today(), "due": date(self.day + 60), "photos": [], "repayments": [],
                    "remind": false}));
            }
            1 | 2 => {
                if let Some((id, mut l)) = self.pick_local(i, "hisab", "loans", |_| true) {
                    let r = json!({"id": format!("r{}", self.seq), "date": self.today(), "amount": self.rng.money(100.0, 2000.0), "note": ""});
                    match l.get_mut("repayments").and_then(Value::as_array_mut) {
                        Some(v) => v.push(r),
                        None => l["repayments"] = json!([r]),
                    }
                    self.put(i, "hisab", "loans", Some(&id), l);
                }
            }
            3 => {
                if let Some(mut g) = self.local(i, "hisab", "goal-trip") {
                    let c = json!({"id": format!("c{}", self.seq), "date": self.today(), "amount": self.rng.money(500.0, 5000.0), "note": ""});
                    match g.get_mut("contributions").and_then(Value::as_array_mut) {
                        Some(v) => v.push(c),
                        None => g["contributions"] = json!([c]),
                    }
                    self.put(i, "hisab", "goals", Some("goal-trip"), g);
                }
            }
            4 => {
                let cat = self.rng.pick(&["cat-food", "cat-fun", "cat-shop", "cat-transport"]);
                if let Some(mut c) = self.local(i, "hisab", cat) {
                    c["budget"] = json!((self.rng.range(20, 200) * 100) as f64);
                    self.put(i, "hisab", "categories", Some(cat), c);
                }
            }
            _ => {
                if let Some(mut r) = self.local(i, "hisab", "hisab-rates") {
                    let mut m = r["manual"].as_object().cloned().unwrap_or_default();
                    let cur = self.rng.pick(&["BDT", "EUR", "INR"]);
                    if m.contains_key(cur) && self.rng.chance(0.5) {
                        m.remove(cur);
                    } else {
                        m.insert(cur.into(), json!(self.rng.money(0.5, 120.0)));
                    }
                    r["manual"] = Value::Object(m);
                    self.put(i, "hisab", "rates", Some("hisab-rates"), r);
                }
            }
        }
    }

    // ---- chrono ----

    fn pc_sessions(&mut self, slot: usize, n: i64) {
        let mut t = sim_ms(self.day, SLOT_MIN[slot], self.tz);
        for _ in 0..n {
            let len = self.rng.range(1, 25) * 60_000 + self.rng.range(0, 59_000);
            if self.rng.chance(0.65) {
                let app = self.rng.pick(&["code", "firefox", "kitty", "slack", "libreoffice", "anki", "thunderbird"]);
                self.put(PC, "chrono", "app_sessions", None, json!({"app": app, "platform": "linux", "start": t, "end": t + len, "tz_offset": self.tz}));
            } else {
                let dom = self.rng.pick(&["github.com", "youtube.com", "news.example.com", "docs.rs", "mail.example.com", "wikipedia.org"]);
                self.put(PC, "chrono", "web_sessions", None, json!({"domain": dom, "browser": "firefox", "device": self.devs[PC].id.clone(), "start": t, "end": t + len, "tz_offset": self.tz}));
            }
            t += len + self.rng.range(0, 10) * 60_000;
        }
    }

    /// The phone importer: yesterday's sessions (deterministic ids, re-imports overwrite),
    /// hourly counters, screen-on intervals for Dusk.
    fn phone_import(&mut self, i: usize) {
        let dev = self.devs[i].id.clone();
        let y = self.day - 1;
        let n = self.rng.range(12, 30);
        let mut t = sim_ms(y, 8 * 60, self.tz);
        let mut ids = vec![];
        for _ in 0..n {
            let len = self.rng.range(1, 30) * 60_000;
            let (label, pkg) = self.rng.pick(&[("WhatsApp", "com.whatsapp"), ("YouTube", "com.google.android.youtube"), ("Chrome", "com.android.chrome"),
                ("Hisab", "com.selfkit.hisab"), ("Maps", "com.google.android.apps.maps"), ("Camera", "com.android.camera")]);
            let id = format!("a-{dev}-{t}-{}", fnv(pkg));
            self.put(i, "chrono", "app_sessions", Some(&id), json!({"app": label, "package": pkg, "platform": "android", "device": dev, "start": t, "end": t + len, "tz_offset": self.tz}));
            ids.push(id);
            t += len + self.rng.range(5, 60) * 60_000;
        }
        // Re-import of the last few: same content again.
        for id in ids.iter().rev().take(3).cloned().collect::<Vec<_>>() {
            if let Some(v) = self.local(i, "chrono", &id) {
                self.put(i, "chrono", "app_sessions", Some(&id), v);
            }
        }
        for h in [8, 12, 18, 22] {
            let hour = sim_ms(y, h * 60, self.tz);
            let (yy, mm, dd) = civil(hour.div_euclid(86_400_000));
            let id = format!("e-{dev}-{yy:04}{mm:02}{dd:02}{:02}", (hour.rem_euclid(86_400_000)) / 3_600_000);
            let mut notif = Map::new();
            notif.insert("com.whatsapp".into(), json!(self.rng.range(0, 20)));
            self.put(i, "chrono", "android_events", Some(&id), json!({"hour": hour, "tz_offset": self.tz, "device": dev, "unlocks": self.rng.range(1, 15), "pickups": self.rng.range(1, 20), "notifications": notif}));
        }
        // Screen-on intervals for last night (Dusk), merged into one record per night.
        let nid = format!("u-{dev}-{}", self.today());
        let mut iv: Vec<Value> = self.local(i, "dusk", &nid).and_then(|v| v["intervals"].as_array().cloned()).unwrap_or_default();
        let s = sim_ms(y, 22 * 60 + self.rng.range(0, 90), self.tz);
        iv.push(json!([s, s + self.rng.range(2, 30) * 60_000]));
        self.put(i, "dusk", "device_use", Some(&nid), json!({"night": self.today(), "device": dev, "platform": "android", "tz_offset": self.tz, "intervals": iv, "source": "usage_stats"}));
    }

    fn day_note(&mut self, i: usize) {
        let id = format!("n-{}", self.today());
        let cur = self.local(i, "chrono", &id);
        if let Some(mut v) = cur.clone()
            && self.rng.chance(0.15)
        {
            v["text"] = json!("");
            self.put(i, "chrono", "day_notes", Some(&id), v);
            self.del(i, "chrono", &id, true);
            if self.rng.chance(0.5) {
                self.del(i, "chrono", &id, false);
            }
            return;
        }
        let text = match cur.as_ref().and_then(|v| v["text"].as_str()).filter(|s| !s.is_empty()) {
            Some(t) if t.len() < 900 => format!("{t}\n{} at {}", self.rng.pick(&["Walked", "Read", "Met a friend", "Long meeting", "Cooked"]), self.rng.range(6, 23)),
            Some(t) => t.to_string(),
            None => format!("Day {}: {}", self.today(), self.rng.pick(&["quiet", "busy", "productive", "tired"])),
        };
        let photos = cur.as_ref().map(|v| v["photos"].clone()).unwrap_or(json!([]));
        self.put(i, "chrono", "day_notes", Some(&id), json!({"day": self.today(), "text": text, "photos": photos, "updated_at": sim_ms(self.day, 21 * 60, self.tz)}));
    }

    fn chrono_settings(&mut self, i: usize, slot: usize) {
        let mut s = self.local(i, "chrono", "settings").unwrap_or(json!({"paused_until": null, "exclude_apps": [], "exclude_domains": [], "categories": {}, "goals": [], "alerts": true}));
        match self.rng.below(5) {
            0 | 1 => {
                // Pause / resume tracking (frequent edits of one record).
                let now = self.now_sim(slot);
                if s["paused_until"].is_null() {
                    s["paused_until"] = json!(now + 3_600_000);
                } else {
                    let from = s["paused_until"].as_i64().unwrap_or(now) - 3_600_000;
                    let mut log: Vec<Value> = s["pause_log"].as_array().cloned().unwrap_or_default();
                    log.push(json!([from, now]));
                    if log.len() > 50 {
                        log.remove(0);
                    }
                    s["pause_log"] = json!(log);
                    s["paused_until"] = Value::Null;
                }
            }
            2 => {
                let mut m = s["categories"].as_object().cloned().unwrap_or_default();
                let k = self.rng.pick(&["code", "anki", "youtube.com", "com.whatsapp", "slack", "github.com", "kitty"]);
                m.insert(k.into(), json!(self.rng.pick(&["work", "study", "social", "entertainment", "other"])));
                s["categories"] = Value::Object(m);
            }
            3 => {
                let mut g: Vec<Value> = s["goals"].as_array().cloned().unwrap_or_default();
                if !g.is_empty() && self.rng.chance(0.4) {
                    let k = self.rng.below(g.len() as u64) as usize;
                    if self.rng.chance(0.5) {
                        g.remove(k);
                    } else {
                        let e = g[k]["enabled"].as_bool().unwrap_or(true);
                        g[k]["enabled"] = json!(!e);
                    }
                } else {
                    g.push(json!({"id": format!("g{}", self.seq), "kind": self.rng.pick(&["limit", "goal"]), "scope": "category", "target": "entertainment", "minutes": self.rng.range(3, 12) * 10, "enabled": true}));
                }
                s["goals"] = json!(g);
            }
            _ => {
                let mut x: Vec<Value> = s["exclude_apps"].as_array().cloned().unwrap_or_default();
                let a = json!(self.rng.pick(&["keepassxc", "com.example.bank", "signal", "steam"]));
                if x.contains(&a) { x.retain(|v| v != &a) } else { x.push(a) }
                s["exclude_apps"] = json!(x);
            }
        }
        self.put(i, "chrono", "chrono_settings", Some("settings"), s);
    }

    // ---- dusk ----

    fn dusk_morning(&mut self, i: usize) {
        let id = format!("n-{}", self.today());
        let mut v = self.local(i, "dusk", &id).unwrap_or(json!({"night": self.today(), "kind": "main", "bed": null, "wake": null, "tz_offset": self.tz,
            "quality": null, "note_before": null, "note_after": null, "created_at": sim_ms(self.day, 7 * 60, self.tz)}));
        v["quality"] = json!(self.rng.range(1, 5));
        if self.rng.chance(0.3) {
            v["note_after"] = json!(self.rng.pick(&["woke at 3", "slept well", "noisy street", "dreams"]));
        }
        if self.rng.chance(0.2) {
            let bed = sim_ms(self.day - 1, 23 * 60 + self.rng.range(0, 90), self.tz);
            v["bed"] = json!(bed);
            v["wake"] = json!(bed + self.rng.range(300, 540) * 60_000);
        }
        self.put(i, "dusk", "sleep_log", Some(&id), v);
    }
    fn dusk_evening(&mut self, i: usize) {
        // "Before bed" note is filed under tomorrow's night (same deterministic id the
        // phone uses in the morning).
        let night = date(self.day + 1);
        let id = format!("n-{night}");
        let mut v = self.local(i, "dusk", &id).unwrap_or(json!({"night": night, "kind": "main", "bed": null, "wake": null, "tz_offset": self.tz,
            "quality": null, "note_before": null, "note_after": null, "created_at": sim_ms(self.day, 22 * 60, self.tz)}));
        v["note_before"] = json!(self.rng.pick(&["tea late", "screen until 1", "workout", "calm evening"]));
        self.put(i, "dusk", "sleep_log", Some(&id), v);
    }
    fn dusk_nap(&mut self, i: usize) {
        let s = sim_ms(self.day, 14 * 60 + self.rng.range(0, 60), self.tz);
        let id = self.put(i, "dusk", "sleep_log", None, json!({"night": self.today(), "kind": "nap", "bed": s, "wake": s + self.rng.range(15, 90) * 60_000, "tz_offset": self.tz,
            "quality": null, "note_before": null, "note_after": null, "created_at": s}));
        if self.rng.chance(0.15) {
            self.del(i, "dusk", &id, true);
            if self.rng.chance(0.5) {
                self.del(i, "dusk", &id, false);
            }
        }
    }
    fn dusk_settings(&mut self, i: usize) {
        let mut s = self.local(i, "dusk", "settings").unwrap_or(json!({"goal_min": 480, "bedtime": "23:00", "winddown_min": 30}));
        match self.rng.below(3) {
            0 => s["goal_min"] = json!(self.rng.range(14, 18) * 30),
            1 => s["bedtime"] = json!(format!("{:02}:{:02}", self.rng.pick(&[22, 23, 0]), self.rng.pick(&[0, 30]))),
            _ => s["winddown_min"] = json!(self.rng.pick(&[15, 30, 45])),
        }
        self.put(i, "dusk", "dusk_settings", Some("settings"), s);
    }

    // ---- reflect + shared settings ----

    fn reflect_settings(&mut self, i: usize) {
        let mut s = self.local(i, "reflect", "settings").unwrap_or(json!({"streaks": [
            {"id": "phone", "kind": "phone_under", "value": 180, "on": true}, {"id": "budget", "kind": "budget", "value": 0, "on": true},
            {"id": "sleep", "kind": "asleep_by", "value": 0, "on": true}, {"id": "tasks", "kind": "tasks_min", "value": 3, "on": true}],
            "lookback_days": 30, "week_start": 0}));
        match self.rng.below(4) {
            0 => s["lookback_days"] = json!(self.rng.pick(&[30, 90, 365])),
            1 => s["week_start"] = json!(self.rng.pick(&[0, 6])),
            2 => {
                let mut v: Vec<Value> = s["streaks"].as_array().cloned().unwrap_or_default();
                v.push(json!({"id": format!("s-{}", self.seq), "kind": "screen_under", "value": 300, "on": true}));
                s["streaks"] = json!(v);
            }
            _ => {
                let mut v: Vec<Value> = s["streaks"].as_array().cloned().unwrap_or_default();
                if !v.is_empty() {
                    let k = self.rng.below(v.len() as u64) as usize;
                    if v[k]["id"].as_str().unwrap_or("").starts_with("s-") && self.rng.chance(0.5) {
                        v.remove(k);
                    } else {
                        let on = v[k]["on"].as_bool().unwrap_or(true);
                        v[k]["on"] = json!(!on);
                    }
                }
                s["streaks"] = json!(v);
            }
        }
        self.put(i, "reflect", "reflect_settings", Some("settings"), s);
    }

    fn shared_settings(&mut self, i: usize, edit: impl FnOnce(&mut Value, &mut Rng)) {
        let mut s = self.local(i, "settings", SETTINGS_ID).unwrap_or_else(|| serde_json::to_value(selfkit_core::settings::Settings::default()).unwrap());
        edit(&mut s, &mut self.rng);
        self.put(i, "settings", "settings", Some(SETTINGS_ID), s);
    }

    // ---- sync ----

    fn away_from_net(&self, _i: usize) -> bool {
        false
    }
    fn at_home(&self, i: usize) -> bool {
        match i {
            A => !self.away,
            _ => true,
        }
    }

    async fn pc_pass(&mut self) {
        if !self.devs[PC].active || !self.pc_on || self.server_down {
            return;
        }
        let t = Instant::now();
        let r = match selfkit_core::sync::pg::PgRemote::connect(&self.srv.pg_url).await {
            Ok(r) => r,
            Err(e) => {
                self.finding("sync failed", "agent", "", "", "", "ok".into(), e.to_string(), "PC agent can't connect".into());
                return;
            }
        };
        for s in STORES {
            let res = {
                let st = self.devs[PC].stores.get_mut(s).unwrap();
                selfkit_core::sync::sync_once(st, &r, &self.key).await.map_err(|e| e.to_string())
            };
            if let Err(msg) = res {
                self.finding("sync failed", s, "", "", "", "ok".into(), msg, "PC agent server pass".into());
            }
        }
        *self.passes.entry("pc agent server".into()).or_default() += 1;
        let y = self.year();
        self.pass_ms.entry(format!("y{y} pc agent server (6 DBs)")).or_default().push(t.elapsed().as_secs_f64() * 1000.0);
        self.after_pass(PC, 0.8).await;
    }

    async fn phone_pass(&mut self, i: usize, app: &str, force_server: bool) -> Option<SyncOutcome> {
        if !self.devs[i].active {
            return None;
        }
        let lan = !force_server && self.at_home(i) && self.pc_on && self.devs[PC].active;
        if self.devs[i].lan_on != Some(lan) {
            let s0 = &self.devs[i].syncs[0].1;
            let mut s = s0.settings();
            s.lan_enabled = lan;
            s0.set_settings(&s).unwrap();
            self.devs[i].lan_on = Some(lan);
        }
        let t = Instant::now();
        let o = self.devs[i].sync_of(app).sync_now(Trigger::Manual).await;
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let tr = match o.transport {
            Some(Transport::Lan) => "lan",
            Some(Transport::Server) => "server",
            Some(Transport::Agent) => "agent",
            None => "none",
        };
        *self.passes.entry(format!("phone {tr}{}", if o.ok { "" } else { " (failed)" })).or_default() += 1;
        let y = self.year();
        self.pass_ms.entry(format!("y{y} phone pass ({tr})")).or_default().push(ms);
        if !o.ok && !(self.server_down && !lan) {
            let name = self.devs[i].name.clone();
            self.finding("sync failed", app, "", "", "", "ok".into(), format!("{:?} / {:?}", o.error, o.detail), format!("device {name}, lan={lan}"));
        }
        if !o.ran && o.ok {
            let name = self.devs[i].name.clone();
            self.finding("sync did not run", app, "", "", "", "ran".into(), format!("{:?}", o.note), format!("device {name}"));
        }
        self.after_pass(i, 0.7).await;
        Some(o)
    }

    /// Look at new conflicts on a device: classify, and (often) settle them like a user would.
    async fn after_pass(&mut self, i: usize, resolve_p: f64) {
        let resolve = self.rng.chance(resolve_p);
        for s in STORES {
            let cs = self.devs[i].st(s).conflicts().unwrap();
            for c in cs {
                let key = format!("{}|{}|{}", self.devs[i].name, c.local.id, c.remote.ver());
                if self.seen_conflicts.insert(key) {
                    self.classify_conflict(i, s, &c);
                }
                if resolve {
                    self.resolve(i, s, &c);
                }
            }
        }
    }

    fn classify_conflict(&mut self, i: usize, store: &str, c: &selfkit_core::store::Conflict) {
        let (l, r) = (&c.local, &c.remote);
        let empty = Map::new();
        let (lm, rm) = (l.data.as_object().unwrap_or(&empty), r.data.as_object().unwrap_or(&empty));
        let diff: Vec<&String> = lm.keys().chain(rm.keys()).collect::<BTreeSet<_>>().into_iter().filter(|k| lm.get(*k) != rm.get(*k)).collect();
        let long = |v: Option<&Value>| matches!(v, Some(Value::String(s)) if s.contains('\n') || s.chars().count() > 80);
        let hist_full = l.hist.len() >= selfkit_core::store::HIST_LEN || r.hist.len() >= selfkit_core::store::HIST_LEN;
        let shared = l.hist.iter().any(|v| r.hist.contains(v));
        let relay = l.device_id != self.devs[i].id;
        let kind = if l.deleted != r.deleted {
            "delete vs edit"
        } else if l.fclock.is_empty() && r.fclock.is_empty() {
            "legacy (no field clocks)"
        } else if diff.iter().any(|k| long(lm.get(*k)) && long(rm.get(*k))) {
            "long text changed on both"
        } else if l.fclock.is_empty() || r.fclock.is_empty() {
            "mixed legacy/new clocks"
        } else if diff.is_empty() {
            "same content"
        } else {
            "other (tie?)"
        };
        let mut label = format!("{store}/{} · {kind}", l.collection);
        if !shared {
            label.push_str(" · no shared history");
        }
        if hist_full {
            label.push_str(" · history full");
        }
        if relay {
            label.push_str(" · local copy not edited here");
        }
        *self.conflicts.entry(label.clone()).or_default() += 1;
        let unexpected = matches!(kind, "other (tie?)" | "same content" | "mixed legacy/new clocks") || (relay && !shared && kind != "delete vs edit");
        if unexpected {
            let note = format!(
                "device {} source {} local ver {} remote ver {} hist {}/{} fields {:?}",
                self.devs[i].name, c.source, l.ver(), r.ver(), l.hist.len(), r.hist.len(), diff
            );
            self.finding(&format!("unexpected conflict: {kind}"), store, &l.collection, &l.id, "", "auto-merge or fast-forward".into(), label, note);
        }
    }

    fn resolve(&mut self, i: usize, store: &str, c: &selfkit_core::store::Conflict) {
        let pick = self.rng.below(10);
        tick();
        if self.devs[i].pc {
            let how = if pick < 5 { Resolution::KeepLocal } else { Resolution::KeepRemote };
            self.devs[i].st(store).resolve_conflict(&c.local.id, how).unwrap();
        } else {
            let choice = match pick {
                0..=3 => Choice::KeepA,
                4..=7 => Choice::KeepB,
                _ => {
                    // Merge: other side's short fields, every line of long text from both.
                    let views = self.devs[i].sync_of(store).conflicts().unwrap();
                    let v = views.into_iter().find(|v| v.id == c.local.id && v.store == store);
                    let mut fields = HashMap::new();
                    let mut lines = HashMap::new();
                    if let Some(v) = v {
                        for f in v.fields {
                            if let Some(ls) = f.lines {
                                lines.insert(f.key.clone(), (0..ls.len()).collect::<BTreeSet<usize>>());
                            } else {
                                fields.insert(f.key.clone(), "b".to_string());
                            }
                        }
                    }
                    Choice::Merge { fields, lines }
                }
            };
            self.devs[i].sync_of(store).resolve(store, &c.local.id, &choice).unwrap();
        }
        after_op();
        let rec = self.devs[i].st(store).get(&c.local.id).unwrap().unwrap();
        if !self.devs[i].new_build {
            self.devs[i].st(store).conn().execute("UPDATE records SET fclock='{}' WHERE id=?", [&rec.id]).unwrap();
        }
        *self.conflicts.entry("(resolved by the user)".into()).or_default() += 1;
        self.o.adopt(&rec);
    }

    async fn resolve_all(&mut self) {
        for i in 0..self.devs.len() {
            if self.devs[i].active {
                for s in STORES {
                    let cs = self.devs[i].st(s).conflicts().unwrap();
                    for c in cs {
                        let key = format!("{}|{}|{}", self.devs[i].name, c.local.id, c.remote.ver());
                        if self.seen_conflicts.insert(key) {
                            self.classify_conflict(i, s, &c);
                        }
                        self.resolve(i, s, &c);
                    }
                }
            }
        }
    }

    fn phones(&self) -> Vec<usize> {
        (1..self.devs.len()).filter(|&i| self.devs[i].active).collect()
    }

    // ---- checks ----

    fn dump(st: &Store) -> BTreeMap<String, (String, Value, bool)> {
        let mut q = st.conn().prepare("SELECT id, collection, data, deleted FROM records").unwrap();
        q.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or(Value::Null), r.get::<_, i64>(3)? != 0)))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    }

    async fn converge_and_check(&mut self, label: &str) {
        self.checks += 1;
        let was_down = self.server_down;
        self.server_down = false;
        for i in self.phones() {
            self.devs[i].sync_of("agenda").set_server(Some(&self.srv.clone())).unwrap();
        }
        let pc_was = self.pc_on;
        self.pc_on = true;
        for _round in 0..4 {
            self.resolve_all().await;
            self.pc_pass().await;
            for i in self.phones() {
                for app in APPS {
                    self.phone_pass(i, app, true).await;
                }
            }
        }
        self.resolve_all().await;
        self.pc_pass().await;
        for i in self.phones() {
            for app in APPS {
                self.phone_pass(i, app, true).await;
            }
        }
        self.pc_on = pc_was;
        // Phones now point at the real server; an ongoing outage is re-applied next day.
        let _ = was_down;
        // Pending, conflicts, identical content.
        let pc_dumps: HashMap<String, BTreeMap<String, (String, Value, bool)>> = STORES.iter().map(|s| (s.to_string(), Self::dump(self.devs[PC].st(s)))).collect();
        for i in 0..self.devs.len() {
            if !self.devs[i].active {
                continue;
            }
            let name = self.devs[i].name.clone();
            for s in STORES {
                let st = self.devs[i].st(s);
                let pend = st.dirty_count().unwrap();
                let nconf = st.conflicts().unwrap().len();
                if pend > 0 {
                    self.finding("changes stuck pending after full sync", s, "", "", "", "0".into(), pend.to_string(), format!("device {name}, check {label}"));
                }
                if nconf > 0 {
                    self.finding("conflicts left after full sync", s, "", "", "", "0".into(), nconf.to_string(), format!("device {name}, check {label}"));
                }
                if i == PC {
                    continue;
                }
                let d = Self::dump(self.devs[i].st(s));
                let p = &pc_dumps[s];
                if &d != p {
                    let mut ids: Vec<&String> = d.keys().chain(p.keys()).collect::<BTreeSet<_>>().into_iter().filter(|k| d.get(*k) != p.get(*k)).collect();
                    ids.truncate(5);
                    let n_diff = d.keys().chain(p.keys()).collect::<BTreeSet<_>>().into_iter().filter(|k| d.get(*k) != p.get(*k)).count();
                    for id in ids {
                        let (a, b) = (d.get(id), p.get(id));
                        let coll = a.or(b).map(|x| x.0.clone()).unwrap_or_default();
                        self.finding(
                            "devices did not converge",
                            s,
                            &coll,
                            id,
                            "",
                            format!("pc: {}", b.map(|x| format!("del={} {}", x.2, short(&x.1))).unwrap_or("missing".into())),
                            format!("{name}: {}", a.map(|x| format!("del={} {}", x.2, short(&x.1))).unwrap_or("missing".into())),
                            format!("{n_diff} records differ, check {label}"),
                        );
                    }
                }
            }
        }
        // Oracle: nothing lost or resurrected.
        let mut todo = vec![];
        for (id, r) in &self.o.recs {
            let store = if r.app == "settings" { "settings" } else { r.app.as_str() };
            match pc_dumps[store].get(id) {
                None => todo.push((id.clone(), r.app.clone(), r.coll.clone(), vec![("record missing".to_string(), "_".to_string(), "exists".to_string(), "missing".to_string())], None)),
                Some((_, data, del)) => {
                    let d = diff_record(r, data, *del);
                    if !d.is_empty() {
                        todo.push((id.clone(), r.app.clone(), r.coll.clone(), d, Some((data.clone(), *del, r.legacy, r.created.clone()))));
                    }
                }
            }
        }
        todo.sort_by(|a, b| a.0.cmp(&b.0));
        for (id, app, coll, diffs, actual) in todo {
            for (kind, field, exp, act) in diffs {
                let note = actual.as_ref().map(|a| format!("record created {}{}, check {label}", a.3, if a.2 { " by an old build (no field clocks)" } else { "" })).unwrap_or_default();
                self.finding(&kind, &app, &coll, &id, &field, exp, act, note);
            }
            if let Some((data, del, _, _)) = actual {
                self.o.reset_to(&id, &data, del);
            }
        }
        for s in STORES {
            for (id, (coll, _, _)) in &pc_dumps[s] {
                if coll != selfkit_core::store::DEVICES && !self.o.recs.contains_key(id) {
                    let (s, coll, id) = (s.to_string(), coll.clone(), id.clone());
                    self.finding("record nobody created", &s, &coll, &id, "", "absent".into(), "present".into(), format!("check {label}"));
                }
            }
        }
        self.duplicates_check(&pc_dumps["hisab"]);
        self.time_queries();
        println!("LONGRUN check {label} (day {} {}): {} findings so far, {} records in model", self.di, self.today(), self.finding_keys.values().sum::<u64>(), self.o.recs.len());
        self.write_report();
    }

    fn duplicates_check(&mut self, hisab: &BTreeMap<String, (String, Value, bool)>) {
        let mut seen: HashMap<(String, String), Vec<String>> = HashMap::new();
        for (id, (coll, v, del)) in hisab {
            if coll == "transactions" && !del && v["source"] == "recurring" {
                seen.entry((v["recurring"].as_str().unwrap_or("").into(), v["date"].as_str().unwrap_or("").into())).or_default().push(id.clone());
            }
        }
        let mut dups: Vec<_> = seen.into_iter().filter(|(_, v)| v.len() > 1).collect();
        dups.sort();
        for ((rid, d), ids) in dups {
            let key = format!("dup|{rid}|{d}");
            if self.seen_conflicts.insert(key) {
                self.finding("recurring transaction generated twice", "hisab", "transactions", &ids.join(","), "recurring", format!("1 for {rid} on {d}"), ids.len().to_string(), "two devices generated the same occurrence".into());
            }
        }
    }

    fn time_queries(&mut self) {
        let y = self.year();
        let day = self.day;
        let tz = self.tz;
        let mut rec = |name: &str, f: &mut dyn FnMut() -> usize, out: &mut BTreeMap<(i64, String), Vec<f64>>| {
            for _ in 0..3 {
                let t = Instant::now();
                let n = f();
                std::hint::black_box(n);
                out.entry((y, name.to_string())).or_default().push(t.elapsed().as_secs_f64() * 1000.0);
            }
        };
        let pc = &self.devs[PC];
        let (h, a, c, d) = (pc.st("hisab"), pc.st("agenda"), pc.st("chrono"), pc.st("dusk"));
        let month_s = date(day);
        let month = &month_s[..7];
        let mut q = BTreeMap::new();
        rec("hisab: month transactions + totals", &mut || {
            let mut s = h.conn().prepare_cached("SELECT data FROM records WHERE collection='transactions' AND deleted=0 AND json_extract(data,'$.date') LIKE ?1").unwrap();
            let rows: Vec<String> = s.query_map([format!("{month}%")], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
            let mut tot: HashMap<String, f64> = HashMap::new();
            for r in &rows {
                let v: Value = serde_json::from_str(r).unwrap();
                *tot.entry(v["category"].as_str().unwrap_or("").into()).or_default() += v["amount"].as_f64().unwrap_or(0.0);
            }
            tot.len() + rows.len()
        }, &mut q);
        rec("hisab: all-time balances (every transaction)", &mut || {
            let mut s = h.conn().prepare_cached("SELECT data FROM records WHERE collection='transactions' AND deleted=0").unwrap();
            let mut bal: HashMap<String, f64> = HashMap::new();
            let rows = s.query_map([], |r| r.get::<_, String>(0)).unwrap();
            for r in rows {
                let v: Value = serde_json::from_str(&r.unwrap()).unwrap();
                *bal.entry(v["account"].as_str().unwrap_or("").into()).or_default() += v["amount"].as_f64().unwrap_or(0.0);
            }
            bal.len()
        }, &mut q);
        rec("agenda: open tasks list", &mut || {
            let mut s = a.conn().prepare_cached("SELECT data FROM records WHERE collection='tasks' AND deleted=0 AND json_extract(data,'$.done_at') IS NULL").unwrap();
            s.query_map([], |r| r.get::<_, String>(0)).unwrap().count()
        }, &mut q);
        rec("agenda: search 'passport'", &mut || a.search(Some("tasks"), "passport", 50).unwrap().len(), &mut q);
        rec("agenda: completions last 30 days", &mut || {
            let from = sim_ms(day - 30, 0, tz);
            let mut s = a.conn().prepare_cached("SELECT count(*) FROM records WHERE collection='completions' AND deleted=0 AND json_extract(data,'$.done_at') >= ?1").unwrap();
            s.query_row([from], |r| r.get::<_, i64>(0)).unwrap() as usize
        }, &mut q);
        rec("chrono: today's sessions", &mut || {
            let (f, t) = (sim_ms(day, 0, tz), sim_ms(day + 1, 0, tz));
            let mut s = c.conn().prepare_cached("SELECT data FROM records WHERE collection IN ('app_sessions','web_sessions') AND deleted=0 AND json_extract(data,'$.start') >= ?1 AND json_extract(data,'$.start') < ?2").unwrap();
            s.query_map([f, t], |r| r.get::<_, String>(0)).unwrap().count()
        }, &mut q);
        rec("chrono: 30-day trend (all sessions in range, per day)", &mut || {
            let (f, t) = (sim_ms(day - 30, 0, tz), sim_ms(day + 1, 0, tz));
            let mut s = c.conn().prepare_cached("SELECT data FROM records WHERE collection IN ('app_sessions','web_sessions') AND deleted=0 AND json_extract(data,'$.start') >= ?1 AND json_extract(data,'$.start') < ?2").unwrap();
            let mut per: HashMap<i64, i64> = HashMap::new();
            for r in s.query_map([f, t], |r| r.get::<_, String>(0)).unwrap() {
                let v: Value = serde_json::from_str(&r.unwrap()).unwrap();
                let st = v["start"].as_i64().unwrap_or(0);
                *per.entry((st + v["tz_offset"].as_i64().unwrap_or(0) * 60_000).div_euclid(86_400_000)).or_default() += v["end"].as_i64().unwrap_or(st) - st;
            }
            per.len()
        }, &mut q);
        rec("dusk: last 90 nights", &mut || {
            let mut s = d.conn().prepare_cached("SELECT data FROM records WHERE collection='sleep_log' AND deleted=0 AND json_extract(data,'$.night') >= ?1").unwrap();
            s.query_map([date(day - 90)], |r| r.get::<_, String>(0)).unwrap().count()
        }, &mut q);
        for (k, v) in q {
            self.q_ms.entry(k).or_default().extend(v);
        }
    }

    fn sizes(&mut self, label: &str) {
        let mut line = format!("| {label} |");
        for i in 0..self.devs.len() {
            let dv = &self.devs[i];
            let mut parts = vec![];
            for s in STORES {
                let _ = dv.st(s).conn().execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
                let b = std::fs::metadata(dv.dir.join(format!("{s}.db"))).map(|m| m.len()).unwrap_or(0);
                parts.push(format!("{s} {:.1}", b as f64 / 1_048_576.0));
            }
            line.push_str(&format!(" {}{}: {} MB |", dv.name, if dv.active { "" } else { " (retired)" }, parts.join(", ")));
        }
        self.sizes.push(line);
    }

    fn write_report(&mut self) {
        let mut r = String::new();
        r.push_str(&format!("# Long-run simulation report\n\nSeed {} · {} simulated days ({} → {}) · reached day {} ({}) · {} checks · {} user writes · {} records in the model\n\n",
            std::env::var("SELFKIT_LONGRUN_SEED").unwrap_or("1".into()), self.n, date(self.start), date(self.start + self.n - 1), self.di, self.today(), self.checks, self.seq, self.o.recs.len()));
        r.push_str("## Findings (category | app/collection.field → count)\n\n");
        if self.finding_keys.is_empty() {
            r.push_str("None.\n");
        }
        let mut fk: Vec<_> = self.finding_keys.iter().collect();
        fk.sort();
        for (k, n) in fk {
            r.push_str(&format!("- **{k}** → {n}\n"));
            for f in self.findings.iter().filter(|f| format!("{} | {}/{}.{}", f.category, f.app, f.collection, f.field) == *k).take(3) {
                r.push_str(&format!("  - day {} ({}) id `{}` expected `{}` actual `{}` {}\n", f.day, f.date, f.id, f.expected, f.actual, f.note));
            }
        }
        r.push_str("\n## Conflicts shown to the user (store/collection · kind)\n\n");
        for (k, n) in &self.conflicts {
            r.push_str(&format!("- {k}: {n}\n"));
        }
        r.push_str("\n## Sync passes\n\n");
        for (k, n) in &self.passes {
            r.push_str(&format!("- {k}: {n}\n"));
        }
        r.push_str("\n| pass time (ms) | n | p50 | p95 | max |\n|---|---|---|---|---|\n");
        for (k, v) in self.pass_ms.clone() {
            let mut v = v;
            let n = v.len();
            r.push_str(&format!("| {k} | {n} | {:.0} | {:.0} | {:.0} |\n", pct(&mut v, 0.5), pct(&mut v, 0.95), pct(&mut v, 1.0)));
        }
        r.push_str("\n## Query times on the PC data (ms, Store-level SQL like the apps run)\n\n| year | query | n | p50 | p95 | max |\n|---|---|---|---|---|---|\n");
        for ((y, k), v) in self.q_ms.clone() {
            let mut v = v;
            let n = v.len();
            r.push_str(&format!("| {y} | {k} | {n} | {:.1} | {:.1} | {:.1} |\n", pct(&mut v, 0.5), pct(&mut v, 0.95), pct(&mut v, 1.0)));
        }
        r.push_str("\n## Database sizes\n\n");
        for l in &self.sizes {
            r.push_str(l);
            r.push('\n');
        }
        let _ = std::fs::write(self.out.join("report.md"), r);
    }

    // ---- one simulated day ----

    async fn live_day(&mut self) {
        let ev_day = self.di;
        let wd = weekday(self.day);
        let weekend = wd == 0 || wd == 6;
        let (_, mon, dom) = civil(self.day);
        // Trips: two weeks over the European DST change each year (Mar 20 – Apr 3), or a
        // short one in short runs; PC off over the year-end holidays.
        let in_trip = (mon == 3 && dom >= 20) || (mon == 4 && dom <= 3) || self.ev.mini_trip.map(|(a, b)| ev_day >= a && ev_day <= b).unwrap_or(false);
        self.away = in_trip;
        self.tz = if in_trip {
            let (y, _, _) = civil(self.day);
            let mut ls = days_from_civil(y, 3, 31);
            while weekday(ls) != 0 {
                ls -= 1;
            }
            let mini_switch = self.ev.mini_trip.map(|(a, b)| ev_day >= (a + b) / 2).unwrap_or(false);
            if self.day >= ls || mini_switch { 120 } else { 60 }
        } else {
            HOME_TZ
        };
        self.pc_on = !((mon == 12 && dom >= 24) || (mon == 1 && dom <= 2));
        let b_off = self.ev.b_offline.iter().any(|(a, b)| ev_day >= *a && ev_day <= *b);
        let down = ev_day >= self.ev.outage.0 && ev_day <= self.ev.outage.1;
        if down != self.server_down {
            self.server_down = down;
            let cfg = if down { self.dead.clone() } else { self.srv.clone() };
            for i in self.phones() {
                self.devs[i].sync_of("agenda").set_server(Some(&cfg)).unwrap();
            }
        }
        let phones = self.phones();
        for slot in 0..SLOT_MIN.len() {
            // Who is using what in this slot.
            let mut users: Vec<usize> = vec![];
            if phones.contains(&A) && self.rng.chance(0.75) {
                users.push(A);
            }
            if !in_trip && self.pc_on && self.devs[PC].active && ((!weekend && (1..=4).contains(&slot)) || (slot >= 5 && self.rng.chance(0.5))) {
                users.push(PC);
            }
            for &p in &phones {
                if p != A && self.rng.chance(if p == B { 0.35 } else { 0.25 }) && !(in_trip && self.rng.chance(0.7)) {
                    users.push(p);
                }
            }
            for &u in &users {
                let ops = self.rng.range(1, 5);
                for _ in 0..ops {
                    let x = self.rng.below(100);
                    match x {
                        0..=17 => self.hisab_tx(u, slot),
                        18..=21 => self.hisab_edit(u),
                        22 => self.hisab_delete(u),
                        23..=25 => self.hisab_misc(u),
                        26..=36 => self.agenda_new(u, slot),
                        37..=48 => self.agenda_complete(u, slot),
                        49..=55 => self.agenda_edit(u),
                        56 => self.agenda_delete(u),
                        57..=58 => self.agenda_focus(u, slot),
                        59..=61 => self.day_note(u),
                        62..=63 => self.chrono_settings(u, slot),
                        64 if self.rng.chance(0.2) => self.dusk_settings(u),
                        65 if self.rng.chance(0.2) => self.reflect_settings(u),
                        66 if self.rng.chance(0.1) => self.shared_settings(u, |s, r| {
                            s["theme"]["mode"] = json!({"mode": "system"});
                            if r.chance(0.5) {
                                s["shortcuts"]["add_task"] = json!(r.pick(&["Super+Shift+T", "Ctrl+Alt+T"]));
                            } else {
                                s["theme"]["light"] = json!(r.pick(&["calm_light", "paper_mono"]));
                            }
                        }),
                        67..=69 if slot == 6 => self.dusk_evening(u),
                        70 if slot >= 3 && slot <= 4 && self.rng.chance(0.3) => self.dusk_nap(u),
                        _ => {}
                    }
                }
                if u == PC && (1..=4).contains(&slot) {
                    let n = self.rng.range(4, 10);
                    self.pc_sessions(slot, n);
                }
            }
            if slot == 0 {
                // Morning: the phone imports yesterday and rates the night.
                if phones.contains(&A) && self.rng.chance(0.9) {
                    self.phone_import(A);
                    self.dusk_morning(A);
                } else if let Some(&p) = phones.iter().find(|&&p| p != A) {
                    self.dusk_morning(p);
                }
            }
            // Sync at irregular times.
            if self.rng.chance(0.85) {
                self.pc_pass().await;
            }
            for &p in &phones {
                if p == B && b_off {
                    continue;
                }
                let prob = if users.contains(&p) { 0.7 } else { 0.15 };
                if self.rng.chance(prob) {
                    let n = self.rng.range(1, 5) as usize;
                    let start = self.rng.below(5) as usize;
                    for k in 0..n {
                        let app = APPS[(start + k) % APPS.len()];
                        self.phone_pass(p, app, false).await;
                    }
                }
            }
        }
    }

    async fn events(&mut self) {
        let d = self.di;
        let e = self.ev.clone();
        if d == e.upgrade_a {
            self.devs[A].new_build = true;
        }
        if d == e.upgrade_pc {
            self.devs[PC].new_build = true;
        }
        if d == e.upgrade_b {
            self.devs[B].new_build = true;
        }
        if d == e.pass_change {
            self.devs[A].syncs[0].1.change_passphrase(PASS1, PASS2).unwrap();
            self.passphrase = PASS2.into();
            println!("LONGRUN day {d}: passphrase changed on phone-a");
        }
        if d == e.day_start.0 {
            self.shared_settings(A, |s, _| s["day_start_min"] = json!(240));
        }
        if d == e.day_start.1 {
            self.shared_settings(PC, |s, _| s["day_start_min"] = json!(0));
        }
        if d == e.currency {
            self.shared_settings(PC, |s, _| s["main_currency"] = json!("BDT"));
        }
        if d == e.c_join {
            // New tablet: passphrase unlock (key file from the server), full first sync.
            let mut c = new_phone(&self.root, "tablet-c", &self.srv, self.pc_addr);
            c.new_build = true;
            let pass = self.passphrase.clone();
            if let Err(err) = c.syncs[0].1.unlock(&pass).await {
                self.finding("unlock failed", "keys", "", "", "", "unlocked".into(), err.to_string(), "new tablet, current passphrase".into());
                c.secrets.set(secrets::DATA_KEY, &b64(&self.key)).unwrap();
            }
            self.devs.push(c);
            let i = self.devs.len() - 1;
            for app in APPS {
                self.phone_pass(i, app, true).await;
            }
            self.converge_and_check("tablet joined").await;
        }
        if d == e.export {
            // Monthly backup bundles from phone A.
            let dir = self.out.join("backups");
            std::fs::create_dir_all(&dir).unwrap();
            for app in ["hisab", "agenda"] {
                let f = std::fs::File::create(dir.join(format!("{app}-{}.skb", self.today()))).unwrap();
                selfkit_core::bundle::export(self.devs[A].st(app), &self.key, 0, i64::MAX, &[], std::io::BufWriter::new(f)).unwrap();
            }
        }
        if d == e.d_restore || d == e.stale_import {
            let dir = self.out.join("backups");
            let date_of = date(self.start + e.export);
            if d == e.d_restore {
                // Old laptop restored from the backup bundle, then syncs with the server.
                let mut dv = new_phone(&self.root, "restore-d", &self.srv, self.pc_addr);
                dv.new_build = true;
                dv.secrets.set(secrets::DATA_KEY, &b64(&self.key)).unwrap();
                for app in ["hisab", "agenda"] {
                    let f = std::fs::File::open(dir.join(format!("{app}-{date_of}.skb"))).unwrap();
                    let (_, rep) = selfkit_core::bundle::import(dv.st(app), &self.key, f, &dv.dir.join("import-files")).unwrap();
                    println!("LONGRUN day {d}: restore-d imported {app}: {} written, {} skipped", rep.written, rep.skipped);
                }
                self.devs.push(dv);
                let i = self.devs.len() - 1;
                for app in APPS {
                    self.phone_pass(i, app, true).await;
                }
                self.converge_and_check("device restored from bundle").await;
                self.devs[i].active = false;
            } else {
                // The user imports an old backup by mistake on phone A: nothing newer may change.
                for app in ["hisab", "agenda"] {
                    let f = std::fs::File::open(dir.join(format!("{app}-{date_of}.skb"))).unwrap();
                    let (_, rep) = selfkit_core::bundle::import(self.devs[A].st(app), &self.key, f, &self.devs[A].dir.join("import-files")).unwrap();
                    if rep.written > 0 {
                        self.finding("old backup import overwrote newer data", app, "", "", "", "0 written".into(), format!("{} written", rep.written), format!("bundle from {date_of}"));
                    }
                }
                self.converge_and_check("stale backup imported").await;
            }
        }
        if d == e.b_lost {
            // Phone B is lost after a final sync; a new phone restores with the recovery key.
            for app in APPS {
                self.phone_pass(B, app, true).await;
            }
            self.devs[B].active = false;
            let mut b2 = new_phone(&self.root, "phone-b2", &self.srv, self.pc_addr);
            b2.new_build = true;
            let rk = self.recovery.clone();
            if let Err(err) = b2.syncs[0].1.unlock_recovery(&rk).await {
                self.finding("recovery key unlock failed", "keys", "", "", "", "unlocked".into(), err.to_string(), "after a passphrase change".into());
                b2.secrets.set(secrets::DATA_KEY, &b64(&self.key)).unwrap();
            }
            self.devs.push(b2);
            let i = self.devs.len() - 1;
            for app in APPS {
                self.phone_pass(i, app, true).await;
            }
            self.converge_and_check("phone replaced (recovery key)").await;
        }
    }
}

// ---------------------------------------------------------------- the test

#[tokio::test]
#[ignore]
async fn longrun_years_of_use() {
    let Ok(days) = std::env::var("SELFKIT_LONGRUN_DAYS") else {
        eprintln!("SELFKIT_LONGRUN_DAYS not set; skipped");
        return;
    };
    let n: i64 = days.parse().expect("SELFKIT_LONGRUN_DAYS");
    let seed: u64 = std::env::var("SELFKIT_LONGRUN_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let end = std::env::var("SELFKIT_LONGRUN_END").ok().and_then(|s| parse_date(&s)).unwrap_or_else(|| days_from_civil(2026, 11, 20));
    let out = std::env::var("SELFKIT_LONGRUN_OUT").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join(format!("selfkit-longrun-{seed}")));
    let _ = std::fs::remove_dir_all(&out);
    let root = out.join("devices");
    std::fs::create_dir_all(&root).unwrap();
    let srv = server().await;
    let dead = ServerConfig { pg_url: format!("postgres://x:y@127.0.0.1:{}/none?sslmode=disable", free_port()), s3: None };
    let f = |x: f64| ((n as f64) * x) as i64;
    let ev = Ev {
        upgrade_a: f(0.04).max(1),
        upgrade_pc: f(0.045).max(1),
        upgrade_b: f(0.06).max(2),
        pass_change: f(0.1).max(3),
        outage: (f(0.3), f(0.3) + (n / 50).clamp(1, 7)),
        b_offline: vec![(f(0.2), f(0.2) + (n / 40).clamp(2, 21)), (f(0.8), f(0.8) + (n / 40).clamp(2, 21))],
        c_join: f(0.4).max(5),
        export: f(0.5),
        d_restore: f(0.55),
        stale_import: f(0.62),
        b_lost: f(0.7),
        day_start: (f(0.35), f(0.45)),
        currency: f(0.47),
        mini_trip: if n < 300 { Some((f(0.64), f(0.64) + 4)) } else { None },
    };

    // Day 0: phone A sets up encryption; the PC agent gets the key; phone B unlocks with
    // the passphrase (key file from the server).
    let tmp_a = new_phone(&root, "phone-a", &srv, SocketAddr::from(([127, 0, 0, 1], 9)));
    let rk = tmp_a.syncs[0].1.setup_encryption_with(PASS1, selfkit_core::keys::KdfParams::insecure_fast()).unwrap();
    let key = tmp_a.syncs[0].1.data_key().unwrap().unwrap().0;
    let (pc, pc_addr) = new_pc(&root, &key).await;
    pc.secrets.set(secrets::DATA_KEY, &b64(&key)).unwrap();
    {
        let s0 = &tmp_a.syncs[0].1;
        let mut s = s0.settings();
        s.lan_peers = vec![pc_addr.to_string()];
        s0.set_settings(&s).unwrap();
    }
    let mut sim = Sim {
        rng: Rng(seed),
        o: Oracle::default(),
        devs: vec![pc, tmp_a],
        srv: srv.clone(),
        dead,
        key: key.clone(),
        pc_addr,
        root: root.clone(),
        out: out.clone(),
        start: end - n + 1,
        n,
        di: 0,
        day: end - n + 1,
        tz: HOME_TZ,
        ev,
        findings: vec![],
        finding_keys: HashMap::new(),
        conflicts: BTreeMap::new(),
        seen_conflicts: HashSet::new(),
        passes: BTreeMap::new(),
        pass_ms: BTreeMap::new(),
        q_ms: BTreeMap::new(),
        sizes: vec![],
        recovery: rk,
        passphrase: PASS1.into(),
        server_down: false,
        pc_on: true,
        away: false,
        seq: 0,
        checks: 0,
    };
    sim.shared_settings(A, |s, _| s["main_currency"] = json!("USD"));
    sim.hisab_setup(A);
    for app in APPS {
        sim.phone_pass(A, app, true).await; // publishes the key file
    }
    let b = new_phone(&root, "phone-b", &srv, pc_addr);
    b.syncs[0].1.unlock(PASS1).await.expect("phone B unlocks with the passphrase");
    sim.devs.push(b);
    sim.pc_pass().await;
    for app in APPS {
        sim.phone_pass(B, app, true).await;
    }

    let t0 = Instant::now();
    for di in 0..n {
        sim.di = di;
        sim.day = sim.start + di;
        sim.events().await;
        sim.live_day().await;
        if di % 30 == 29 || di == n - 1 {
            sim.converge_and_check(&format!("day {di}")).await;
        }
        if di % 365 == 364 || di == n - 1 {
            sim.sizes(&format!("day {di} ({})", date(sim.day)));
            let c = pg_size(&srv.pg_url).await;
            sim.sizes.push(format!("| day {di} server sk_records | {:.1} MB |", c as f64 / 1_048_576.0));
        }
        if di % 100 == 0 {
            println!("LONGRUN day {di} {} after {:.0}s, {} writes", date(sim.day), t0.elapsed().as_secs_f64(), sim.seq);
        }
    }
    // Leave the data folders ready for the real apps: sync off (test keys), WAL checkpointed.
    for d in &sim.devs {
        for s in STORES {
            let _ = d.st(s).conn().execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
        }
        if let Some((_, s0)) = d.syncs.first() {
            let mut s = s0.settings();
            s.enabled = false;
            let _ = s0.set_settings(&s);
        }
    }
    let readme = format!(
        "Selfkit long-run simulation data ({} days ending {}, seed {seed}).\nOne folder per device under devices/ (pc = desktop agent data; phone-a, phone-b(2), tablet-c are phones).\nOpen with the real apps: SELFKIT_DATA_DIR=<folder> ./agenda (or hisab, chrono, dusk, reflect).\nSync is turned off in these folders. Test-only passphrase: \"{}\"; recovery key: {}\nreport.md has findings, conflict counts, query times and sizes.\n",
        n, date(end), sim.passphrase, sim.recovery
    );
    std::fs::write(out.join("README.txt"), readme).unwrap();
    sim.write_report();
    println!("LONGRUN done: {} days in {:.0}s, {} writes, {} findings ({} categories). Report: {}", n, t0.elapsed().as_secs_f64(), sim.seq,
        sim.finding_keys.values().sum::<u64>(), sim.finding_keys.len(), out.join("report.md").display());
    let mut fk: Vec<_> = sim.finding_keys.iter().collect();
    fk.sort();
    for (k, v) in fk {
        println!("LONGRUN category {k}: {v}");
    }
    if std::env::var("SELFKIT_LONGRUN_STRICT").as_deref() == Ok("1") {
        assert!(sim.finding_keys.is_empty(), "long-run findings; see report.md");
    }
}

async fn pg_size(url: &str) -> i64 {
    let Ok((c, conn)) = tokio_postgres::connect(url, tokio_postgres::NoTls).await else { return 0 };
    tokio::spawn(conn);
    c.query_one("SELECT pg_total_relation_size('sk_records')", &[]).await.map(|r| r.get::<_, i64>(0)).unwrap_or(0)
}
