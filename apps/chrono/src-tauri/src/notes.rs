//! Daily summary notes with photos. Docs: docs/apps/chrono/notes.md.
//!
//! `day_notes` (id `n-<YYYY-MM-DD>`, one per local day): `{day, text, photos: [photo id], updated_at}`.
//! Photos are scaled by the UI to document quality (longest side ≤ 2400 px,
//! WebP, JPEG fallback) and stored as files in `<data_dir>/chrono-photos/`
//! (not in SQLite); collection `photos` holds `{file, mime, w, h, bytes, sha256, created_at}`.
//! Same scheme as Agenda's photos.

use std::path::PathBuf;

use selfkit_core::{Error, Result, Store, paths, time};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

pub const NOTES: &str = "day_notes";
pub const PHOTOS: &str = "photos";
pub const MAX_BYTES: usize = 12 * 1024 * 1024;
pub const MAX_TEXT: usize = 1000;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DayNote {
    pub day: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub photos: Vec<String>,
    #[serde(default)]
    pub updated_at: i64,
}

pub fn note_id(day: &str) -> String {
    format!("n-{day}")
}

pub fn get(store: &Store, day: &str) -> Result<Option<DayNote>> {
    Ok(match store.get(&note_id(day))? {
        Some(r) if !r.deleted => serde_json::from_value(r.data).ok(),
        _ => None,
    })
}

/// Save (or delete when empty) the note for a day.
pub fn save(store: &Store, day: &str, text: &str, photos: &[String]) -> Result<Option<DayNote>> {
    if crate::stats::str_to_day(day).is_none() {
        return Err(Error::Format("bad day".into()));
    }
    let text: String = text.trim().chars().take(MAX_TEXT).collect();
    if text.is_empty() && photos.is_empty() {
        if store.get(&note_id(day))?.is_some_and(|r| !r.deleted) {
            store.delete(&note_id(day))?;
        }
        return Ok(None);
    }
    let n = DayNote { day: day.into(), text, photos: photos.to_vec(), updated_at: time::now_ms() };
    store.put(NOTES, Some(&note_id(day)), &serde_json::to_value(&n)?)?;
    Ok(Some(n))
}

pub fn dir() -> PathBuf {
    paths::data_dir().join("chrono-photos")
}

fn ext(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "image/webp" => "webp",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        _ => return None,
    })
}

pub fn save_photo(store: &Store, bytes: &[u8], mime: &str, w: u32, h: u32) -> Result<String> {
    let ext = ext(mime).ok_or_else(|| Error::Format(format!("Unsupported image type {mime}")))?;
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(Error::Format("That photo is empty or too large.".into()));
    }
    let id = selfkit_core::store::new_id();
    let file = format!("{id}.{ext}");
    std::fs::create_dir_all(dir())?;
    let tmp = dir().join(format!("{file}.part"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, dir().join(&file))?;
    let sha = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect::<String>();
    store.put(
        PHOTOS,
        Some(&id),
        &json!({"file": file, "mime": mime, "w": w, "h": h, "bytes": bytes.len(), "sha256": sha, "created_at": time::now_ms()}),
    )?;
    Ok(id)
}

/// (mime, bytes) of a photo.
pub fn load_photo(store: &Store, id: &str) -> Result<(String, Vec<u8>)> {
    let r = store.get(id)?.filter(|r| r.collection == PHOTOS).ok_or_else(|| Error::NotFound(id.into()))?;
    let file = r.data["file"].as_str().ok_or_else(|| Error::NotFound(id.into()))?;
    if file.contains('/') || file.contains('\\') || file.contains("..") {
        return Err(Error::Format("bad photo file name".into()));
    }
    let mime = r.data["mime"].as_str().unwrap_or("image/webp").to_string();
    Ok((mime, std::fs::read(dir().join(file))?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_and_photos() {
        let tmp = std::env::temp_dir().join(format!("chrono-notes-{}", std::process::id()));
        paths::set_data_dir(tmp.clone());
        let s = Store::open_in_memory("chrono", "d", &[]).unwrap();
        let pid = save_photo(&s, b"RIFFxxxxWEBP", "image/webp", 10, 10).unwrap();
        assert!(save_photo(&s, b"x", "image/gif", 1, 1).is_err());
        let n = save(&s, "2026-11-14", "  A good day  ", std::slice::from_ref(&pid)).unwrap().unwrap();
        assert_eq!(n.text, "A good day");
        assert_eq!(get(&s, "2026-11-14").unwrap().unwrap().photos, vec![pid.clone()]);
        assert_eq!(load_photo(&s, &pid).unwrap().1, b"RIFFxxxxWEBP");
        assert!(save(&s, "nope", "x", &[]).is_err());
        assert!(save(&s, "2026-11-14", " ", &[]).unwrap().is_none());
        assert!(get(&s, "2026-11-14").unwrap().is_none());
        let _ = std::fs::remove_dir_all(tmp);
    }
}
