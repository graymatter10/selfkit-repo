//! Startup safety: open a DB even when its file is damaged, and keep one
//! desktop instance. (Same file in agenda, hisab and dusk; QA 2026-11-15.)
//!
//! A file SQLite can't read ("not a database", "malformed") used to make the
//! setup hook fail, so the app closed at once with no message, forever. Now the
//! damaged file (plus its -wal/-shm) is moved aside as `<name>.damaged-<ms>`
//! (kept, never deleted), a fresh DB is created, and the UI gets a notice.
//! Other errors (locked, disk full, permissions) are returned unchanged.

use std::path::{Path, PathBuf};

use selfkit_core::{Store, store::Migration};

/// True for errors that mean the file itself is unreadable as a database.
pub fn is_damaged(err: &selfkit_core::Error) -> bool {
    let m = err.to_string();
    m.contains("malformed") || m.contains("not a database") || m.contains("file is encrypted")
}

/// Open `path`; if the file is damaged, move it aside and start fresh.
/// Returns the store and, after a recovery, a sentence for the user.
pub fn open(path: PathBuf, app: &str, device: &str, migs: &[Migration]) -> selfkit_core::Result<(Store, Option<String>)> {
    match Store::open(&path, app, device, migs) {
        Ok(s) => Ok((s, None)),
        Err(e) if is_damaged(&e) => {
            let aside = set_aside(&path)?;
            eprintln!("{}: damaged ({e}); moved to {}", path.display(), aside.display());
            let s = Store::open(&path, app, device, migs)?;
            // Short: it shows in a one-line toast. The kept file's name is in the log.
            Ok((s, Some("Damaged data file set aside · started fresh".into())))
        }
        Err(e) => Err(e),
    }
}

fn set_aside(path: &Path) -> std::io::Result<PathBuf> {
    let stamp = selfkit_core::time::now_ms();
    let aside = path.with_file_name(format!("{}.damaged-{stamp}", path.file_name().unwrap_or_default().to_string_lossy()));
    std::fs::rename(path, &aside)?;
    for ext in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{ext}", path.display()));
        if side.exists() {
            let _ = std::fs::rename(&side, format!("{}{ext}", aside.display()));
        }
    }
    Ok(aside)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damaged_file_is_set_aside() {
        let dir = std::env::temp_dir().join(format!("sk-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("x.db");
        std::fs::write(&db, b"this is not a database at all, just text that is long enough").unwrap();
        let (s, note) = open(db.clone(), "x", "dev", &[]).unwrap();
        assert!(note.unwrap().contains("Damaged"));
        s.put("c", None, &serde_json::json!({"a": 1})).unwrap();
        let kept: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().contains(".damaged-")).collect();
        assert_eq!(kept.len(), 1);
        drop(s);
        let (_, note) = open(db, "x", "dev", &[]).unwrap();
        assert!(note.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
