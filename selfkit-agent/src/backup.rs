//! Automatic backups: a monthly encrypted `.skb` bundle per app (records changed
//! in that UTC month) and a weekly full export of every app, uploaded to S3 and
//! optionally kept locally. Plus `restore`. See docs/components/selfkit-agent.md
//! § Backups.
//!
//! S3 layout: `bundles/<app>/monthly/<YYYY-MM>-<dev8>.skb`,
//! `bundles/<app>/weekly/<YYYY-Www>-<dev8>.skb`, `bundles/_keys/keys.json`
//! (the passphrase/recovery-wrapped key file, safe to store anywhere).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{Datelike, TimeZone, Utc};
use selfkit_core::crypto::SecretKey;
use selfkit_core::sync::s3::{BlobStore, S3Config};
use selfkit_core::{Store, bundle, keychain, paths, time};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::config::BackupCfg;

/// Every DB that gets backed up.
pub fn backup_apps() -> impl Iterator<Item = &'static str> {
    paths::APPS.iter().copied().chain(std::iter::once(selfkit_core::settings::APP))
}

/// S3 settings: env (`SELFKIT_S3_ENDPOINT`, `_BUCKET`, `_REGION`, `_KEY`, `_SECRET`)
/// for dev/test, else the keychain entry `s3-config` (JSON `S3Config`).
pub fn s3_config() -> Result<Option<S3Config>> {
    if let Ok(endpoint) = std::env::var("SELFKIT_S3_ENDPOINT") {
        let v = |k: &str| std::env::var(k).with_context(|| format!("{k} not set"));
        return Ok(Some(S3Config {
            endpoint,
            bucket: v("SELFKIT_S3_BUCKET")?,
            region: std::env::var("SELFKIT_S3_REGION").unwrap_or_else(|_| "us-east-1".into()),
            access_key: v("SELFKIT_S3_KEY")?,
            secret_key: v("SELFKIT_S3_SECRET")?,
            path_style: std::env::var("SELFKIT_S3_PATH_STYLE").as_deref() != Ok("0"),
        }));
    }
    match keychain::get(keychain::S3_CONFIG) {
        Ok(Some(s)) => Ok(Some(serde_json::from_str(&s).context("keychain s3-config is not valid JSON")?)),
        _ => Ok(None),
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BundleInfo {
    pub app: String,
    pub kind: String,
    pub label: String,
    pub path: String,
    pub records: u64,
    pub bytes: u64,
}

/// Persisted in `<data_dir>/backups/state.json`; also shown in health.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupState {
    /// app → last finished monthly label ("YYYY-MM").
    pub monthly: BTreeMap<String, String>,
    /// Last finished weekly label ("YYYY-Www").
    pub weekly: Option<String>,
    pub last_run_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    pub last_error: Option<String>,
    /// "s3", "local" or "s3+local".
    pub target: Option<String>,
    pub last_bundles: Vec<BundleInfo>,
}

pub fn backups_dir() -> PathBuf {
    paths::data_dir().join("backups")
}

pub fn state_path() -> PathBuf {
    backups_dir().join("state.json")
}

impl BackupState {
    pub fn load() -> Self {
        std::fs::read(state_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(backups_dir())?;
        let tmp = backups_dir().join("state.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, state_path())?;
        Ok(())
    }
}

// ---- time labels (UTC) ----------------------------------------------------

pub fn month_label(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms).single().unwrap_or_default().format("%Y-%m").to_string()
}

fn parse_month(label: &str) -> Option<(i32, u32)> {
    let (y, m) = label.split_once('-')?;
    let (y, m) = (y.parse().ok()?, m.parse().ok()?);
    (1..=12).contains(&m).then_some((y, m))
}

fn next_month((y, m): (i32, u32)) -> (i32, u32) {
    if m == 12 { (y + 1, 1) } else { (y, m + 1) }
}

/// [from, to) of a "YYYY-MM" label in UTC ms.
pub fn month_range(label: &str) -> Option<(i64, i64)> {
    let (y, m) = parse_month(label)?;
    let (ny, nm) = next_month((y, m));
    let from = Utc.with_ymd_and_hms(y, m, 1, 0, 0, 0).single()?.timestamp_millis();
    let to = Utc.with_ymd_and_hms(ny, nm, 1, 0, 0, 0).single()?.timestamp_millis();
    Some((from, to))
}

pub fn week_label(ms: i64) -> String {
    let d = Utc.timestamp_millis_opt(ms).single().unwrap_or_default();
    let w = d.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

/// Finished months still to back up: from the month of the oldest record (or the
/// month after `last_done`) up to the month before `now`.
pub fn due_months(earliest: Option<i64>, last_done: Option<&str>, now: i64) -> Vec<String> {
    let Some(earliest) = earliest else { return vec![] };
    let cur = parse_month(&month_label(now)).unwrap();
    let mut m = parse_month(&month_label(earliest)).unwrap();
    if let Some(done) = last_done.and_then(parse_month) {
        m = m.max(next_month(done));
    }
    let mut out = vec![];
    while m < cur && out.len() < 600 {
        out.push(format!("{:04}-{:02}", m.0, m.1));
        m = next_month(m);
    }
    out
}

fn dev8(device: &str) -> String {
    device.chars().filter(|c| c.is_ascii_alphanumeric()).rev().take(8).collect::<Vec<_>>().into_iter().rev().collect()
}

/// Attachments of an app: `<data_dir>/files/<app>/<id>`, optionally only those
/// modified in [from, to).
fn app_files(app: &str, range: Option<(i64, i64)>) -> Vec<(String, PathBuf)> {
    let dir = paths::data_dir().join("files").join(app);
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    rd.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| {
            let Some((from, to)) = range else { return true };
            let m = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
            m.is_some_and(|d| (from..to).contains(&(d.as_millis() as i64)))
        })
        .filter_map(|e| Some((e.file_name().to_str()?.to_string(), e.path())))
        .collect()
}

// ---- targets ----------------------------------------------------------------

struct Target {
    s3: Option<BlobStore>,
    keep_local: bool,
}

impl Target {
    fn name(&self) -> &'static str {
        match (self.s3.is_some(), self.keep_local) {
            (true, true) => "s3+local",
            (true, false) => "s3",
            _ => "local",
        }
    }

    /// Store a finished bundle; returns the S3 path or the local path.
    async fn put(&self, app: &str, kind: &str, name: &str, file: &Path) -> Result<String> {
        let mut out = String::new();
        if self.keep_local {
            let dir = backups_dir().join(app).join(kind);
            std::fs::create_dir_all(&dir)?;
            let dest = dir.join(name);
            std::fs::copy(file, &dest)?;
            out = dest.display().to_string();
        }
        if let Some(s3) = &self.s3 {
            out = s3.put_bundle(app, &format!("{kind}/{name}"), file).await?;
        }
        Ok(out)
    }

    /// Keep only the newest `keep` weekly bundles of this device for `app`.
    async fn prune_weekly(&self, app: &str, device: &str, keep_s3: usize, keep_local: usize) -> Result<()> {
        let suffix = format!("-{}.skb", dev8(device));
        if let Some(s3) = &self.s3 {
            let mut keys: Vec<String> =
                s3.list(&format!("bundles/{app}/weekly/")).await?.into_iter().map(|(k, _)| k).filter(|k| k.ends_with(&suffix)).collect();
            keys.sort();
            let n = keys.len().saturating_sub(keep_s3.max(1));
            for k in &keys[..n] {
                s3.delete_bundle(k).await?;
                tracing::info!(path = %k, "old weekly backup deleted");
            }
        }
        if self.keep_local {
            let dir = backups_dir().join(app).join("weekly");
            if let Ok(rd) = std::fs::read_dir(&dir) {
                let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(&suffix)).collect();
                v.sort();
                let n = v.len().saturating_sub(keep_local.max(1));
                for p in &v[..n] {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
        Ok(())
    }
}

fn tmp_dir() -> Result<PathBuf> {
    let d = backups_dir().join("tmp");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn earliest_update(store: &Store) -> Result<Option<i64>> {
    Ok(store.conn().query_row("SELECT MIN(updated_at) FROM records", [], |r| r.get(0))?)
}

/// Write one bundle to a temp file and hand it to the target.
async fn make_and_put(
    target: &Target,
    store: &Store,
    key: &SecretKey,
    kind: &str,
    label: &str,
    range: (i64, i64),
    files: Vec<(String, PathBuf)>,
) -> Result<Option<BundleInfo>> {
    let app = store.app().to_string();
    let name = format!("{label}-{}.skb", dev8(store.device_id()));
    let tmp = tempfile::NamedTempFile::new_in(tmp_dir()?)?;
    // StreamWriter already writes in 64 KiB chunks, so no BufWriter.
    let manifest = bundle::export(store, key, range.0, range.1, &files, tmp.as_file())?;
    if manifest.records == 0 && manifest.files == 0 {
        return Ok(None);
    }
    let bytes = tmp.as_file().metadata()?.len();
    let path = target.put(&app, kind, &name, tmp.path()).await?;
    tracing::info!(%app, kind, label, records = manifest.records, bytes, "backup stored");
    Ok(Some(BundleInfo { app, kind: kind.into(), label: label.into(), path, records: manifest.records, bytes }))
}

/// Run every backup that is due (or a full export now when `force_full`).
/// Updates and saves the state file and returns it.
pub async fn run_due(cfg: &BackupCfg, key: &SecretKey, force_full: bool) -> BackupState {
    let mut st = BackupState::load();
    let now = time::now_ms();
    st.last_run_at = Some(now);
    st.last_bundles.clear();
    let res = run_inner(cfg, key, force_full, now, &mut st).await;
    match res {
        Ok(()) => {
            st.last_ok_at = Some(now);
            st.last_error = None;
        }
        Err(e) => {
            tracing::warn!("backup failed: {e:#}");
            st.last_error = Some(format!("{e:#}"));
        }
    }
    if let Err(e) = st.save() {
        tracing::warn!("backup state not saved: {e:#}");
    }
    let _ = std::fs::remove_dir_all(backups_dir().join("tmp"));
    st
}

async fn run_inner(cfg: &BackupCfg, key: &SecretKey, force_full: bool, now: i64, st: &mut BackupState) -> Result<()> {
    let s3 = match s3_config()? {
        Some(c) => {
            let bs = BlobStore::new(c)?;
            bs.ensure_bucket().await.context("S3 bucket")?;
            Some(bs)
        }
        None => None,
    };
    anyhow::ensure!(
        s3.is_some() || cfg.keep_local,
        "no backup target: configure S3 (keychain `s3-config` or SELFKIT_S3_*) or turn on keep_local"
    );
    let target = Target { s3, keep_local: cfg.keep_local };
    st.target = Some(target.name().into());
    let device = paths::device_id()?;

    // Monthly archives per app.
    for app in backup_apps() {
        let p = paths::app_db(app);
        if !p.exists() {
            continue;
        }
        let store = Store::open(&p, app, &device, &[])?;
        for label in due_months(earliest_update(&store)?, st.monthly.get(app).map(String::as_str), now) {
            let range = month_range(&label).unwrap();
            if let Some(b) = make_and_put(&target, &store, key, "monthly", &label, range, app_files(app, Some(range))).await? {
                st.last_bundles.push(b);
            }
            st.monthly.insert(app.into(), label);
            st.save()?;
        }
    }

    // Weekly full export of every app.
    let week = week_label(now);
    if force_full || st.weekly.as_deref() != Some(week.as_str()) {
        for app in backup_apps() {
            let p = paths::app_db(app);
            if !p.exists() {
                continue;
            }
            let store = Store::open(&p, app, &device, &[])?;
            if let Some(b) = make_and_put(&target, &store, key, "weekly", &week, (0, i64::MAX), app_files(app, None)).await? {
                st.last_bundles.push(b);
            }
            target.prune_weekly(app, &device, cfg.weekly_keep, cfg.local_weekly_keep).await?;
        }
        // The wrapped key file, so a fresh device can unlock with the recovery key.
        if let (Some(s3), true) = (&target.s3, paths::key_file().exists()) {
            s3.put_bundle("_keys", "keys.json", &paths::key_file()).await?;
        }
        st.weekly = Some(week);
    }
    Ok(())
}

// ---- restore -----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Remote {
    pub app: String,
    pub kind: String,
    pub label: String,
    pub path: String,
    pub size: u64,
}

/// Parse `bundles/<app>/<kind>/<label>-<dev>.skb`.
pub fn parse_remote(path: &str, size: u64) -> Option<Remote> {
    let mut it = path.strip_prefix("bundles/")?.split('/');
    let (app, kind, name) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() || app.starts_with('_') || !matches!(kind, "monthly" | "weekly") {
        return None;
    }
    let stem = name.strip_suffix(".skb")?;
    let (label, _) = stem.rsplit_once('-')?;
    Some(Remote { app: app.into(), kind: kind.into(), label: label.into(), path: path.into(), size })
}

/// What a restore uses per app: the newest weekly full export (any device) and
/// every monthly archive. Import is "newer wins", so order does not matter.
pub fn pick_restore(list: &[Remote], app: Option<&str>) -> BTreeMap<String, Vec<Remote>> {
    let mut out: BTreeMap<String, Vec<Remote>> = BTreeMap::new();
    for r in list.iter().filter(|r| app.is_none_or(|a| a == r.app)) {
        let v = out.entry(r.app.clone()).or_default();
        if r.kind == "monthly" {
            v.push(r.clone());
        } else if let Some(i) = v.iter().position(|x| x.kind == "weekly") {
            if (r.label.as_str(), r.path.as_str()) > (v[i].label.as_str(), v[i].path.as_str()) {
                v[i] = r.clone();
            }
        } else {
            v.push(r.clone());
        }
    }
    for v in out.values_mut() {
        v.sort_by_key(|r| (r.kind != "weekly", r.label.clone()));
    }
    out
}

fn import_file(app: &str, key: &SecretKey, file: &Path) -> Result<(bundle::Manifest, bundle::ImportReport)> {
    let store = Store::open(paths::app_db(app), app, &paths::device_id()?, &[])?;
    let f = std::io::BufReader::new(std::fs::File::open(file)?);
    let files_dir = paths::data_dir().join("files").join(app);
    Ok(bundle::import(&store, key, f, &files_dir)?)
}

/// Import one local `.skb` file (the app comes from its manifest).
pub fn restore_file(key: &SecretKey, file: &Path) -> Result<Value> {
    let m = bundle::read_manifest(key, std::io::BufReader::new(std::fs::File::open(file)?)).context("read bundle (wrong key?)")?;
    let (_, rep) = import_file(&m.app, key, file)?;
    Ok(json!({"app": m.app, "records": m.records, "written": rep.written, "skipped": rep.skipped, "files": rep.files.len()}))
}

/// Download `keys.json` from S3 into the data folder if it is missing locally.
pub async fn fetch_key_file() -> Result<bool> {
    let kp = paths::key_file();
    if kp.exists() {
        return Ok(false);
    }
    let Some(c) = s3_config()? else { return Ok(false) };
    let s3 = BlobStore::new(c)?;
    if !s3.list("bundles/_keys/").await?.iter().any(|(k, _)| k == "bundles/_keys/keys.json") {
        return Ok(false);
    }
    std::fs::create_dir_all(paths::data_dir())?;
    s3.get_bundle("bundles/_keys/keys.json", &kp).await?;
    Ok(true)
}

pub async fn list_remote() -> Result<Vec<Remote>> {
    let c = s3_config()?.context("S3 is not configured (keychain `s3-config` or SELFKIT_S3_*)")?;
    let s3 = BlobStore::new(c)?;
    let mut v: Vec<Remote> = s3.list("bundles/").await?.into_iter().filter_map(|(k, s)| parse_remote(&k, s)).collect();
    v.sort_by(|a, b| (&a.app, &a.kind, &a.label).cmp(&(&b.app, &b.kind, &b.label)));
    Ok(v)
}

/// Restore from S3 into the current data folder (`SELFKIT_DATA_DIR` for a fresh one).
pub async fn restore_s3(key: &SecretKey, app: Option<&str>) -> Result<Value> {
    let c = s3_config()?.context("S3 is not configured")?;
    let s3 = BlobStore::new(c)?;
    let list = list_remote().await?;
    let plan = pick_restore(&list, app);
    anyhow::ensure!(!plan.is_empty(), "no backups found on S3");
    let tmp = tmp_dir()?;
    let mut out = serde_json::Map::new();
    for (app, bundles) in plan {
        let (mut written, mut skipped, mut files) = (0u64, 0u64, 0usize);
        for b in &bundles {
            let f = tmp.join(format!("restore-{}.skb", selfkit_core::store::new_id()));
            s3.get_bundle(&b.path, &f).await?;
            let r = import_file(&app, key, &f);
            let _ = std::fs::remove_file(&f);
            let (_, rep) = r.with_context(|| format!("import {}", b.path))?;
            written += rep.written;
            skipped += rep.skipped;
            files += rep.files.len();
        }
        let used: Vec<&str> = bundles.iter().map(|b| b.path.as_str()).collect();
        out.insert(app, json!({"bundles": used, "written": written, "skipped": skipped, "files": files}));
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(y: i32, m: u32, d: u32) -> i64 {
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap().timestamp_millis()
    }

    #[test]
    fn labels_and_ranges() {
        assert_eq!(month_label(ms(2026, 11, 15)), "2026-11");
        let (a, b) = month_range("2026-12").unwrap();
        assert_eq!(month_label(a), "2026-12");
        assert_eq!(month_label(b), "2027-01");
        assert_eq!(month_label(b - 1), "2026-12");
        assert_eq!(week_label(ms(2026, 11, 15)), "2026-W46");
        assert_eq!(week_label(ms(2027, 1, 1)), "2026-W53");
        assert!(month_range("2026-13").is_none());
    }

    #[test]
    fn due_months_logic() {
        let now = ms(2026, 11, 15);
        assert!(due_months(None, None, now).is_empty());
        assert_eq!(due_months(Some(ms(2026, 8, 3)), None, now), ["2026-08", "2026-09", "2026-10"]);
        assert_eq!(due_months(Some(ms(2026, 8, 3)), Some("2026-09"), now), ["2026-10"]);
        assert!(due_months(Some(ms(2026, 8, 3)), Some("2026-10"), now).is_empty());
        assert!(due_months(Some(ms(2026, 11, 2)), None, now).is_empty());
        assert_eq!(due_months(Some(ms(2025, 12, 31)), None, ms(2026, 2, 1)), ["2025-12", "2026-01"]);
    }

    #[test]
    fn remote_parsing_and_pick() {
        assert_eq!(parse_remote("bundles/chrono/monthly/2026-10-abcd1234.skb", 5).unwrap().label, "2026-10");
        assert_eq!(parse_remote("bundles/chrono/weekly/2026-W46-abcd1234.skb", 5).unwrap().label, "2026-W46");
        assert!(parse_remote("bundles/_keys/keys.json", 5).is_none());
        assert!(parse_remote("bundles/x/other/a-b.skb", 5).is_none());
        assert!(parse_remote("blobs/x", 5).is_none());
        let list: Vec<Remote> = [
            "bundles/chrono/weekly/2026-W45-aaaa.skb",
            "bundles/chrono/weekly/2026-W46-bbbb.skb",
            "bundles/chrono/monthly/2026-09-aaaa.skb",
            "bundles/chrono/monthly/2026-10-aaaa.skb",
            "bundles/agenda/weekly/2026-W44-aaaa.skb",
        ]
        .iter()
        .map(|p| parse_remote(p, 1).unwrap())
        .collect();
        let plan = pick_restore(&list, None);
        let chrono: Vec<&str> = plan["chrono"].iter().map(|r| r.path.as_str()).collect();
        assert_eq!(
            chrono,
            ["bundles/chrono/weekly/2026-W46-bbbb.skb", "bundles/chrono/monthly/2026-09-aaaa.skb", "bundles/chrono/monthly/2026-10-aaaa.skb"]
        );
        assert_eq!(plan["agenda"].len(), 1);
        assert_eq!(pick_restore(&list, Some("agenda")).len(), 1);
    }

    #[test]
    fn dev8_takes_tail() {
        assert_eq!(dev8("0199aaaa-0000-7000-8000-00000000abcd"), "0000abcd");
    }
}
