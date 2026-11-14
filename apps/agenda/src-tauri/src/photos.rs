//! Photos attached to task descriptions. Docs: docs/apps/agenda/photos.md.
//!
//! The UI scales each photo to document quality (longest side ≤ 2400 px, WebP
//! where the webview can encode it, else JPEG) before saving. Bytes are stored
//! as files in `<data_dir>/agenda-photos/<id>.<ext>` (not in SQLite, so the DB
//! stays small); collection `photos` holds the metadata:
//! `file, mime, w, h, bytes, sha256, created_at (UTC ms)`. Tasks reference them by id in `photos`.

use std::path::PathBuf;

use selfkit_core::{Error, Result, Store, paths, time};
use serde_json::json;
use sha2::{Digest, Sha256};

pub const PHOTOS: &str = "photos";
pub const MAX_BYTES: usize = 12 * 1024 * 1024;

pub fn dir() -> PathBuf {
    paths::data_dir().join("agenda-photos")
}

fn ext(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "image/webp" => "webp",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        _ => return None,
    })
}

pub fn save(store: &Store, bytes: &[u8], mime: &str, w: u32, h: u32) -> Result<String> {
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
pub fn load(store: &Store, id: &str) -> Result<(String, Vec<u8>)> {
    let r = store.get(id)?.filter(|r| r.collection == PHOTOS).ok_or_else(|| Error::NotFound(id.into()))?;
    let file = r.data["file"].as_str().ok_or_else(|| Error::NotFound(id.into()))?;
    if file.contains('/') || file.contains("..") {
        return Err(Error::Format("bad photo file name".into()));
    }
    let mime = r.data["mime"].as_str().unwrap_or("image/webp").to_string();
    Ok((mime, std::fs::read(dir().join(file))?))
}
