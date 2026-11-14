//! Receipt and loan-proof photos. The webview shrinks the picked image to at
//! most 2400 px on the long edge and encodes it as high-quality WebP (canvas,
//! quality 0.9); the bytes are stored here as `<data_dir>/hisab-photos/<id>.webp`
//! (JPEG `.jpg` only when the webview cannot encode WebP). Records keep just
//! the photo id. Upload to S3 (encrypted, core `sync::s3::BlobStore`) is not
//! wired yet; see docs/apps/hisab/README.md § Photos.

use std::path::PathBuf;

use selfkit_core::paths;

use crate::model::R;

pub fn dir() -> PathBuf {
    paths::data_dir().join("hisab-photos")
}

fn ext_of(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG") {
        Some("png")
    } else {
        None
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Store image bytes; returns the photo id (`<uuid>.<ext>`).
pub fn save(bytes: &[u8]) -> R<String> {
    let ext = ext_of(bytes).ok_or("Not a WebP, JPEG or PNG image")?;
    if bytes.len() > 25 * 1024 * 1024 {
        return Err("Photo is too large".into());
    }
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let id = format!("{}.{ext}", selfkit_core::store::new_id());
    std::fs::write(dir().join(&id), bytes).map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn load(id: &str) -> R<Vec<u8>> {
    let (stem, ext) = id.rsplit_once('.').ok_or("bad photo id")?;
    if !valid_id(stem) || !["webp", "jpg", "png"].contains(&ext) {
        return Err("bad photo id".into());
    }
    std::fs::read(dir().join(id)).map_err(|_| "Photo not found on this device".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load() {
        let tmp = std::env::temp_dir().join(format!("hisab-photo-test-{}", std::process::id()));
        paths::set_data_dir(tmp.clone());
        let webp = [b"RIFF".as_slice(), &[0, 0, 0, 0], b"WEBPVP8 ", &[1, 2, 3]].concat();
        let id = save(&webp).unwrap();
        assert!(id.ends_with(".webp"));
        assert_eq!(load(&id).unwrap(), webp);
        assert!(save(b"hello").is_err());
        assert!(load("../../etc/passwd.webp").is_err());
        let _ = std::fs::remove_dir_all(tmp);
    }
}
