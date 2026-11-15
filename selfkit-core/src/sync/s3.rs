//! Encrypted blob storage on any S3-compatible service (Backblaze B2, MinIO,
//! SeaweedFS, AWS). Photos are encrypted with the data key before upload.

use rand::Rng;
use s3::{Bucket, BucketConfiguration, Region, creds::Credentials};
use serde::{Deserialize, Serialize};

use crate::crypto::{self, SecretKey};
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    /// Path-style URLs (needed for MinIO/SeaweedFS; B2 works either way).
    #[serde(default = "yes")]
    pub path_style: bool,
}

fn yes() -> bool {
    true
}

fn err(e: impl std::fmt::Display) -> Error {
    Error::Remote(e.to_string())
}

pub struct BlobStore {
    bucket: Box<Bucket>,
    cfg: S3Config,
}

fn blob_path(id: &str) -> Result<String> {
    if id.is_empty() || id.contains(['/', '\\']) || id.contains("..") {
        return Err(Error::Format(format!("bad blob id {id}")));
    }
    Ok(format!("blobs/{id}"))
}

/// Security: `http://` (no TLS) is only accepted for a server on this machine (the
/// local Docker fake). Anything else must be `https://`, so requests, signatures and
/// encrypted blobs can't be read, replayed or swapped on the network. See docs/security.md.
pub fn check_endpoint(endpoint: &str) -> Result<()> {
    let e = endpoint.trim();
    let Some(rest) = e.get(..7).filter(|p| p.eq_ignore_ascii_case("http://")).map(|_| &e[7..]) else {
        return Ok(());
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    let local = host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
    if local {
        Ok(())
    } else {
        Err(Error::Remote(format!("insecure storage endpoint: use https:// for {host} (http:// is only allowed for a server on this machine)")))
    }
}

impl BlobStore {
    pub fn new(cfg: S3Config) -> Result<Self> {
        check_endpoint(&cfg.endpoint)?;
        let bucket = Self::bucket(&cfg)?;
        Ok(Self { bucket, cfg })
    }

    fn region(cfg: &S3Config) -> Region {
        Region::Custom { region: cfg.region.clone(), endpoint: cfg.endpoint.clone() }
    }
    fn creds(cfg: &S3Config) -> Result<Credentials> {
        Credentials::new(Some(&cfg.access_key), Some(&cfg.secret_key), None, None, None).map_err(err)
    }
    fn bucket(cfg: &S3Config) -> Result<Box<Bucket>> {
        let b = Bucket::new(&cfg.bucket, Self::region(cfg), Self::creds(cfg)?).map_err(err)?;
        Ok(if cfg.path_style { b.with_path_style() } else { b })
    }

    /// Create the bucket if it doesn't exist (dev servers; on B2 create it in the console).
    pub async fn ensure_bucket(&self) -> Result<()> {
        if self.bucket.exists().await.map_err(err)? {
            return Ok(());
        }
        let r = if self.cfg.path_style {
            Bucket::create_with_path_style(&self.cfg.bucket, Self::region(&self.cfg), Self::creds(&self.cfg)?, BucketConfiguration::default()).await
        } else {
            Bucket::create(&self.cfg.bucket, Self::region(&self.cfg), Self::creds(&self.cfg)?, BucketConfiguration::default()).await
        }
        .map_err(err)?;
        if !r.success() {
            return Err(Error::Remote(format!("create bucket: {} {}", r.response_code, r.response_text)));
        }
        Ok(())
    }

    async fn put_raw(&self, path: &str, bytes: &[u8]) -> Result<()> {
        let r = self.bucket.put_object(path, bytes).await.map_err(err)?;
        if !(200..300).contains(&r.status_code()) {
            return Err(Error::Remote(format!("put {path}: HTTP {}", r.status_code())));
        }
        Ok(())
    }

    async fn get_raw(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let r = self.bucket.get_object(path).await.map_err(err)?;
        match r.status_code() {
            200..=299 => Ok(Some(r.bytes().to_vec())),
            404 => Ok(None),
            c => Err(Error::Remote(format!("get {path}: HTTP {c}"))),
        }
    }

    /// Encrypt and upload a photo/attachment.
    pub async fn put_blob(&self, key: &SecretKey, id: &str, plaintext: &[u8]) -> Result<()> {
        let ct = crypto::encrypt_stream(key, plaintext, Vec::with_capacity(plaintext.len() + 64))?;
        self.put_raw(&blob_path(id)?, &ct).await
    }

    /// Download and decrypt; `None` if missing.
    pub async fn get_blob(&self, key: &SecretKey, id: &str) -> Result<Option<Vec<u8>>> {
        match self.get_raw(&blob_path(id)?).await? {
            Some(ct) => Ok(Some(crypto::decrypt_stream(key, ct.as_slice(), Vec::new())?)),
            None => Ok(None),
        }
    }

    pub async fn delete_blob(&self, id: &str) -> Result<()> {
        self.bucket.delete_object(blob_path(id)?).await.map_err(err)?;
        Ok(())
    }

    /// Upload an already-encrypted bundle file as `bundles/<app>/<name>`.
    pub async fn put_bundle(&self, app: &str, name: &str, local: &std::path::Path) -> Result<String> {
        let path = format!("bundles/{app}/{name}");
        let mut f = tokio::fs::File::open(local).await?;
        let r = self.bucket.put_object_stream(&mut f, &path).await.map_err(err)?;
        if !(200..300).contains(&r.status_code()) {
            return Err(Error::Remote(format!("put {path}: HTTP {}", r.status_code())));
        }
        Ok(path)
    }

    pub async fn get_bundle(&self, path: &str, local: &std::path::Path) -> Result<()> {
        let mut f = tokio::fs::File::create(local).await?;
        let code = self.bucket.get_object_to_writer(path, &mut f).await.map_err(err)?;
        if !(200..300).contains(&code) {
            return Err(Error::Remote(format!("get {path}: HTTP {code}")));
        }
        // tokio::fs::File buffers the last write; without this the file can end short.
        tokio::io::AsyncWriteExt::flush(&mut f).await?;
        f.sync_all().await?;
        Ok(())
    }

    /// Delete an uploaded bundle (only `bundles/...` paths; used by backup retention).
    pub async fn delete_bundle(&self, path: &str) -> Result<()> {
        if !path.starts_with("bundles/") || path.contains("..") {
            return Err(Error::Format(format!("not a bundle path: {path}")));
        }
        let r = self.bucket.delete_object(path).await.map_err(err)?;
        if !(200..300).contains(&r.status_code()) {
            return Err(Error::Remote(format!("delete {path}: HTTP {}", r.status_code())));
        }
        Ok(())
    }

    pub async fn list(&self, prefix: &str) -> Result<Vec<(String, u64)>> {
        let pages = self.bucket.list(prefix.to_string(), None).await.map_err(err)?;
        Ok(pages.into_iter().flat_map(|p| p.contents).map(|o| (o.key, o.size)).collect())
    }

    /// Connectivity check used by the health screen: write/read/delete a tiny object.
    pub async fn check(&self) -> Result<()> {
        let path = format!("health/{}", rand::thread_rng().r#gen::<u64>());
        self.put_raw(&path, b"ok").await?;
        let back = self.get_raw(&path).await?;
        self.bucket.delete_object(&path).await.map_err(err)?;
        if back.as_deref() != Some(b"ok") {
            return Err(Error::Remote("health object mismatch".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> S3Config {
        let v = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} not set; source dev/test.env"));
        S3Config {
            endpoint: v("SELFKIT_TEST_S3_ENDPOINT"),
            bucket: v("SELFKIT_TEST_S3_BUCKET"),
            region: v("SELFKIT_TEST_S3_REGION"),
            access_key: v("SELFKIT_TEST_S3_KEY"),
            secret_key: v("SELFKIT_TEST_S3_SECRET"),
            path_style: true,
        }
    }

    #[test]
    fn plain_http_only_for_local_endpoints() {
        for ok in ["https://s3.eu-central-003.backblazeb2.com", "http://127.0.0.1:59000", "http://localhost:9000/", "http://[::1]:9000", "HTTPS://x.example"] {
            assert!(check_endpoint(ok).is_ok(), "{ok}");
        }
        for bad in ["http://s3.example.com", "HTTP://192.168.1.10:9000", "http://localhost.evil.com", "http://user@10.0.0.2:9000/b", "http://127.0.0.1.nip.io"] {
            assert!(check_endpoint(bad).is_err(), "{bad}");
        }
        let mut c = S3Config { endpoint: "http://s3.example.com".into(), bucket: "b".into(), region: "r".into(), access_key: "a".into(), secret_key: "s".into(), path_style: true };
        assert!(BlobStore::new(c.clone()).is_err());
        c.endpoint = "https://s3.example.com".into();
        assert!(BlobStore::new(c).is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn s3_blob_roundtrip() {
        let bs = BlobStore::new(cfg()).unwrap();
        bs.ensure_bucket().await.unwrap();
        bs.check().await.unwrap();
        let key = SecretKey::generate();
        let id = uuid::Uuid::now_v7().to_string();
        let photo: Vec<u8> = (0..300_000u32).map(|i| (i % 253) as u8).collect();
        bs.put_blob(&key, &id, &photo).await.unwrap();
        assert_eq!(bs.get_blob(&key, &id).await.unwrap().unwrap(), photo);
        assert!(bs.get_blob(&SecretKey::generate(), &id).await.is_err());
        bs.delete_blob(&id).await.unwrap();
        assert!(bs.get_blob(&key, &id).await.unwrap().is_none());

        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("b.skb");
        std::fs::write(&f, vec![1u8; 6 * 1024 * 1024]).unwrap();
        let path = bs.put_bundle("hisab", &format!("{id}.skb"), &f).await.unwrap();
        assert!(bs.list("bundles/hisab/").await.unwrap().iter().any(|(k, s)| *k == path && *s == 6 * 1024 * 1024));
        let back = dir.path().join("back.skb");
        bs.get_bundle(&path, &back).await.unwrap();
        assert_eq!(std::fs::read(back).unwrap().len(), 6 * 1024 * 1024);
    }
}
