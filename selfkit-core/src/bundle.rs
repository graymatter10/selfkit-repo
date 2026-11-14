//! Selfkit bundle (`.skb`): `SKB1` + encrypted STREAM of a tar holding
//! `manifest.json`, `records.jsonl` and `files/<id>`. See docs/data-and-sync.md.

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::crypto::{SecretKey, StreamReader, StreamWriter};
use crate::store::Record;
use crate::{Error, Result, Store, time};

const MAGIC: &[u8; 4] = b"SKB1";
pub const FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub format: u32,
    pub app: String,
    /// UTC ms range [from, to) of `updated_at`.
    pub from: i64,
    pub to: i64,
    pub schema: Vec<String>,
    pub device_id: String,
    pub created_at: i64,
    pub records: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub written: u64,
    pub skipped: u64,
    pub files: Vec<PathBuf>,
}

fn tar_append<W: Write>(b: &mut tar::Builder<W>, path: &str, size: u64, r: impl Read) -> Result<()> {
    let mut h = tar::Header::new_gnu();
    h.set_size(size);
    h.set_mode(0o600);
    h.set_mtime((time::now_ms() / 1000) as u64);
    b.append_data(&mut h, path, r)?;
    Ok(())
}

/// Export records of `store` changed in [from, to) (tombstones included) plus
/// the given attachment files `(id, path)` into `out`.
pub fn export<W: Write>(store: &Store, key: &SecretKey, from: i64, to: i64, files: &[(String, PathBuf)], mut out: W) -> Result<Manifest> {
    // Records go to a temp file first so RAM stays flat and tar knows the size.
    let mut tmp = tempfile::tempfile()?;
    let mut n = 0u64;
    {
        let mut w = std::io::BufWriter::new(&mut tmp);
        for r in store.list_range(None, from, to, true)? {
            serde_json::to_writer(&mut w, &r)?;
            w.write_all(b"\n")?;
            n += 1;
        }
        w.flush()?;
    }
    let size = tmp.stream_position()?;
    tmp.seek(SeekFrom::Start(0))?;

    let schema: Vec<String> = {
        let mut st = store.conn().prepare("SELECT name FROM schema_migrations ORDER BY name")?;
        st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
    };
    let manifest = Manifest {
        format: FORMAT,
        app: store.app().into(),
        from,
        to,
        schema,
        device_id: store.device_id().into(),
        created_at: time::now_ms(),
        records: n,
        files: files.len() as u64,
    };

    out.write_all(MAGIC)?;
    let sw = StreamWriter::new(key, out)?;
    let mut tb = tar::Builder::new(sw);
    let mj = serde_json::to_vec_pretty(&manifest)?;
    tar_append(&mut tb, "manifest.json", mj.len() as u64, mj.as_slice())?;
    tar_append(&mut tb, "records.jsonl", size, &mut tmp)?;
    for (id, path) in files {
        if id.contains(['/', '\\']) || id.contains("..") {
            return Err(Error::Format(format!("bad file id {id}")));
        }
        let f = std::fs::File::open(path)?;
        let len = f.metadata()?.len();
        tar_append(&mut tb, &format!("files/{id}"), len, f)?;
    }
    let sw = tb.into_inner()?;
    sw.finish()?;
    Ok(manifest)
}

/// Read only the manifest (decrypts just the first entry).
pub fn read_manifest<R: Read>(key: &SecretKey, r: R) -> Result<Manifest> {
    let mut ar = open_archive(key, r)?;
    let mut entries = ar.entries()?;
    let mut e = entries.next().ok_or_else(|| Error::Format("empty bundle".into()))??;
    if e.path()?.to_str() != Some("manifest.json") {
        return Err(Error::Format("manifest must be first".into()));
    }
    let mut buf = Vec::new();
    e.read_to_end(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

fn open_archive<R: Read>(key: &SecretKey, mut r: R) -> Result<tar::Archive<StreamReader<R>>> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).map_err(|_| Error::Format("not a bundle".into()))?;
    if &magic != MAGIC {
        return Err(Error::Format("not a selfkit bundle".into()));
    }
    Ok(tar::Archive::new(StreamReader::new(key, r)?))
}

fn map_io(e: std::io::Error) -> Error {
    if e.kind() == std::io::ErrorKind::InvalidData { Error::Decrypt } else { Error::Io(e) }
}

/// Import a bundle into `store` ("newer wins" per record). Attachments are
/// written to `files_dir/<id>`. Fails if the bundle is for a different app.
pub fn import<R: Read>(store: &Store, key: &SecretKey, r: R, files_dir: &Path) -> Result<(Manifest, ImportReport)> {
    let mut ar = open_archive(key, r)?;
    let mut manifest: Option<Manifest> = None;
    let mut rep = ImportReport::default();
    for e in ar.entries().map_err(map_io)? {
        let mut e = e.map_err(map_io)?;
        let path = e.path()?.to_string_lossy().into_owned();
        if path == "manifest.json" {
            let mut buf = Vec::new();
            e.read_to_end(&mut buf).map_err(map_io)?;
            let m: Manifest = serde_json::from_slice(&buf)?;
            if m.format > FORMAT {
                return Err(Error::Format(format!("bundle format {} is newer than this app", m.format)));
            }
            if m.app != store.app() {
                return Err(Error::Format(format!("bundle is for {}, not {}", m.app, store.app())));
            }
            manifest = Some(m);
        } else if path == "records.jsonl" {
            if manifest.is_none() {
                return Err(Error::Format("manifest must be first".into()));
            }
            let tx = store.conn().unchecked_transaction()?;
            for line in BufReader::new(&mut e).lines() {
                let line = line.map_err(map_io)?;
                if line.is_empty() {
                    continue;
                }
                let rec: Record = serde_json::from_str(&line)?;
                if store.import_record(&rec)? { rep.written += 1 } else { rep.skipped += 1 }
            }
            tx.commit()?;
        } else if let Some(id) = path.strip_prefix("files/") {
            if id.is_empty() || id.contains(['/', '\\']) || id.contains("..") {
                return Err(Error::Format(format!("bad file entry {path}")));
            }
            std::fs::create_dir_all(files_dir)?;
            let dest = files_dir.join(id);
            let mut f = std::fs::File::create(&dest)?;
            std::io::copy(&mut e, &mut f).map_err(map_io)?;
            rep.files.push(dest);
        }
    }
    let m = manifest.ok_or_else(|| Error::Format("missing manifest".into()))?;
    Ok((m, rep))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn export_import_roundtrip() {
        let key = SecretKey::generate();
        let a = Store::open_in_memory("hisab", "dev-a", &[]).unwrap();
        for i in 0..1000 {
            a.put("transactions", None, &json!({"amount": i, "note": "x".repeat(50)})).unwrap();
        }
        let gone = a.put("transactions", None, &json!({"amount": -1})).unwrap();
        a.delete(&gone.id).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("p.webp");
        std::fs::write(&photo, vec![9u8; 200_000]).unwrap();
        let mut buf = Vec::new();
        let m = export(&a, &key, 0, i64::MAX, &[("ph1".into(), photo)], &mut buf).unwrap();
        assert_eq!((m.records, m.files), (1001, 1));
        assert!(!buf.windows(6).any(|w| w == b"amount"), "bundle must be encrypted");
        assert_eq!(read_manifest(&key, buf.as_slice()).unwrap(), m);

        let b = Store::open_in_memory("hisab", "dev-b", &[]).unwrap();
        let (_, rep) = import(&b, &key, buf.as_slice(), &dir.path().join("out")).unwrap();
        assert_eq!((rep.written, rep.skipped), (1001, 0));
        assert_eq!(b.count("transactions").unwrap(), 1000);
        assert!(b.get(&gone.id).unwrap().unwrap().deleted);
        assert_eq!(std::fs::read(&rep.files[0]).unwrap().len(), 200_000);
        // re-import is a no-op
        let (_, rep) = import(&b, &key, buf.as_slice(), &dir.path().join("out")).unwrap();
        assert_eq!(rep.written, 0);
        // wrong app / wrong key rejected
        let c = Store::open_in_memory("tickd", "dev-c", &[]).unwrap();
        assert!(import(&c, &key, buf.as_slice(), dir.path()).is_err());
        assert!(import(&b, &SecretKey::generate(), buf.as_slice(), dir.path()).is_err());
    }
}
