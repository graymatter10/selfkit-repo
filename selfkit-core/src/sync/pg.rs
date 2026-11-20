//! Postgres [`Remote`] — standard SQL only, works on any Postgres ≥ 12
//! (Neon, Aiven, self-hosted). TLS via rustls (webpki roots, hostname checked).
//! Security: TLS is *required* for every server that isn't on this machine,
//! whatever the URL says (`sslmode` missing/`prefer` → `require`; `disable` is
//! refused), so nobody on the network can strip TLS and ask for the password in
//! clear. Only loopback/Unix-socket servers (the local Docker fakes) may run
//! without TLS. See docs/security.md.

use std::str::FromStr;
use std::sync::Arc;

use tokio_postgres::config::{Host, SslMode};
use tokio_postgres::{Client, Config, NoTls};

use super::{Remote, RemoteRecord};
use crate::{Error, Result};

/// Versioned server migrations. Append only; never edit a shipped entry.
pub const MIGRATIONS: &[(i32, &str)] = &[(
    1,
    r#"
CREATE SEQUENCE IF NOT EXISTS sk_rev_seq;
CREATE TABLE IF NOT EXISTS sk_records (
    id          TEXT PRIMARY KEY,
    app         TEXT NOT NULL,
    collection  TEXT NOT NULL,
    updated_at  BIGINT NOT NULL,
    device_id   TEXT NOT NULL,
    ciphertext  BYTEA NOT NULL,
    rev         BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS sk_records_app_rev ON sk_records(app, rev);
CREATE TABLE IF NOT EXISTS sk_meta (k TEXT PRIMARY KEY, v TEXT NOT NULL);
"#,
), (
    2,
    // R-1: ids are only unique per app ("settings" in Chrono, Dusk and Reflect; "n-<date>"
    // in Dusk and Chrono), so rows are keyed by (app, id).
    r#"
ALTER TABLE sk_records DROP CONSTRAINT IF EXISTS sk_records_pkey;
ALTER TABLE sk_records ADD PRIMARY KEY (app, id);
"#,
)];

fn err(e: tokio_postgres::Error) -> Error {
    Error::Remote(e.to_string())
}

pub struct PgRemote {
    client: Client,
}

fn tls() -> tokio_postgres_rustls::MakeRustlsConnect {
    let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("tls versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    tokio_postgres_rustls::MakeRustlsConnect::new(cfg)
}

fn is_local_host(h: &Host) -> bool {
    match h {
        Host::Tcp(name) => {
            let n = name.trim_start_matches('[').trim_end_matches(']');
            n.eq_ignore_ascii_case("localhost") || n.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
        }
        #[cfg(unix)]
        Host::Unix(_) => true,
    }
}

/// Parse a connection string and apply the TLS policy (see module docs).
pub fn secure_config(url: &str) -> Result<Config> {
    let mut cfg = Config::from_str(url).map_err(err)?;
    let local = !cfg.get_hosts().is_empty() && cfg.get_hosts().iter().all(is_local_host);
    match cfg.get_ssl_mode() {
        SslMode::Disable if !local => {
            return Err(Error::Remote(
                "insecure postgres: sslmode=disable is only allowed for a server on this machine; remove it (TLS is required)".into(),
            ));
        }
        SslMode::Disable | SslMode::Require => {}
        _ if local => {}
        // Prefer (the default) would silently fall back to plaintext if a man in the middle says "no TLS".
        _ => {
            cfg.ssl_mode(SslMode::Require);
        }
    }
    Ok(cfg)
}

impl PgRemote {
    /// Connect with a standard connection string (URL or key=value). TLS is required for
    /// non-local servers; `sslmode=disable` (plain TCP) only works for loopback/Unix sockets.
    pub async fn connect(url: &str) -> Result<Self> {
        let cfg = secure_config(url)?;
        let client = if cfg.get_ssl_mode() == SslMode::Disable {
            let (c, conn) = cfg.connect(NoTls).await.map_err(err)?;
            tokio::spawn(async move {
                if let Err(e) = conn.await {
                    tracing::warn!("postgres connection: {e}");
                }
            });
            c
        } else {
            let (c, conn) = cfg.connect(tls()).await.map_err(err)?;
            tokio::spawn(async move {
                if let Err(e) = conn.await {
                    tracing::warn!("postgres connection: {e}");
                }
            });
            c
        };
        let me = Self { client };
        me.migrate().await?;
        Ok(me)
    }

    pub async fn migrate(&self) -> Result<()> {
        self.client
            .batch_execute("CREATE TABLE IF NOT EXISTS sk_schema (version INT PRIMARY KEY, applied_at BIGINT NOT NULL)")
            .await
            .map_err(err)?;
        for (v, sql) in MIGRATIONS {
            let done = self.client.query_opt("SELECT 1 FROM sk_schema WHERE version=$1", &[v]).await.map_err(err)?;
            if done.is_some() {
                continue;
            }
            // Advisory lock so two devices migrating at once don't race.
            self.client.batch_execute("BEGIN; SELECT pg_advisory_xact_lock(726173);").await.map_err(err)?;
            let r = async {
                if self.client.query_opt("SELECT 1 FROM sk_schema WHERE version=$1", &[v]).await?.is_none() {
                    self.client.batch_execute(sql).await?;
                    self.client
                        .execute("INSERT INTO sk_schema(version, applied_at) VALUES ($1, $2)", &[v, &crate::time::now_ms()])
                        .await?;
                }
                self.client.batch_execute("COMMIT").await
            }
            .await;
            if let Err(e) = r {
                let _ = self.client.batch_execute("ROLLBACK").await;
                return Err(err(e));
            }
        }
        Ok(())
    }

    pub async fn meta_get(&self, k: &str) -> Result<Option<String>> {
        Ok(self.client.query_opt("SELECT v FROM sk_meta WHERE k=$1", &[&k]).await.map_err(err)?.map(|r| r.get(0)))
    }

    pub async fn meta_set(&self, k: &str, v: &str) -> Result<()> {
        self.client
            .execute("INSERT INTO sk_meta(k, v) VALUES ($1, $2) ON CONFLICT (k) DO UPDATE SET v=EXCLUDED.v", &[&k, &v])
            .await
            .map_err(err)?;
        Ok(())
    }

    /// Newest schema version this build knows (last entry of [`MIGRATIONS`]).
    pub fn known_schema_version() -> i32 {
        MIGRATIONS.last().map(|m| m.0).unwrap_or(0)
    }

    /// Highest schema version applied on the server. Greater than
    /// [`Self::known_schema_version`] means a newer Selfkit set it up.
    pub async fn server_schema_version(&self) -> Result<i32> {
        let r = self.client.query_one("SELECT COALESCE(MAX(version), 0) FROM sk_schema", &[]).await.map_err(err)?;
        Ok(r.get(0))
    }

    /// Delete every synced row (all apps). Used once when the user moves from the
    /// public test key to their own key: the rows can't be read with the new key,
    /// and every device re-uploads its data after unlocking.
    pub async fn wipe_records(&self) -> Result<u64> {
        self.client.execute("DELETE FROM sk_records", &[]).await.map_err(err)
    }

    /// Database size in bytes (health screen warns at 80% of the plan limit).
    pub async fn size_bytes(&self) -> Result<i64> {
        Ok(self.client.query_one("SELECT pg_database_size(current_database())", &[]).await.map_err(err)?.get(0))
    }
}

fn row(r: &tokio_postgres::Row) -> RemoteRecord {
    RemoteRecord {
        id: r.get(0),
        app: r.get(1),
        collection: r.get(2),
        updated_at: r.get(3),
        device_id: r.get(4),
        ciphertext: r.get(5),
        rev: r.get(6),
    }
}

impl Remote for PgRemote {
    async fn push(&self, rec: &RemoteRecord, base_rev: i64) -> Result<Option<i64>> {
        let r = if base_rev == 0 {
            self.client
                .query_opt(
                    "INSERT INTO sk_records (id, app, collection, updated_at, device_id, ciphertext, rev)
                     VALUES ($1, $2, $3, $4, $5, $6, nextval('sk_rev_seq'))
                     ON CONFLICT (app, id) DO NOTHING RETURNING rev",
                    &[&rec.id, &rec.app, &rec.collection, &rec.updated_at, &rec.device_id, &rec.ciphertext],
                )
                .await
        } else {
            self.client
                .query_opt(
                    "UPDATE sk_records SET collection=$3, updated_at=$4, device_id=$5, ciphertext=$6,
                       rev=nextval('sk_rev_seq')
                     WHERE app=$2 AND id=$1 AND rev=$7 RETURNING rev",
                    &[&rec.id, &rec.app, &rec.collection, &rec.updated_at, &rec.device_id, &rec.ciphertext, &base_rev],
                )
                .await
        }
        .map_err(err)?;
        Ok(r.map(|r| r.get(0)))
    }

    async fn pull(&self, app: &str, since: i64, limit: i64) -> Result<Vec<RemoteRecord>> {
        let rows = self
            .client
            .query(
                "SELECT id, app, collection, updated_at, device_id, ciphertext, rev FROM sk_records
                 WHERE app=$1 AND rev>$2 ORDER BY rev LIMIT $3",
                &[&app, &since, &limit],
            )
            .await
            .map_err(err)?;
        Ok(rows.iter().map(row).collect())
    }

    async fn fetch(&self, app: &str, id: &str) -> Result<Option<RemoteRecord>> {
        let r = self
            .client
            .query_opt(
                "SELECT id, app, collection, updated_at, device_id, ciphertext, rev FROM sk_records WHERE app=$1 AND id=$2",
                &[&app, &id],
            )
            .await
            .map_err(err)?;
        Ok(r.as_ref().map(row))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Store, crypto::SecretKey, sync::sync_once};
    use serde_json::json;

    #[test]
    fn tls_policy() {
        let mode = |u: &str| secure_config(u).map(|c| c.get_ssl_mode());
        // Remote servers: TLS required, whatever the URL asks for.
        assert_eq!(mode("postgres://u:p@db.example.com/x").unwrap(), SslMode::Require);
        assert_eq!(mode("postgres://u:p@db.example.com/x?sslmode=prefer").unwrap(), SslMode::Require);
        assert_eq!(mode("postgres://u:p@db.example.com/x?sslmode=require").unwrap(), SslMode::Require);
        assert!(mode("postgres://u:p@db.example.com/x?sslmode=disable").is_err());
        assert!(mode("host=10.0.0.5 user=u sslmode=disable").is_err());
        // A mixed host list counts as remote.
        assert!(mode("host=localhost,db.example.com user=u sslmode=disable").is_err());
        // Local dev servers (Docker fakes) may run without TLS.
        assert_eq!(mode("postgres://u:p@127.0.0.1:55432/x").unwrap(), SslMode::Prefer);
        assert_eq!(mode("postgres://u:p@localhost/x?sslmode=disable").unwrap(), SslMode::Disable);
        assert_eq!(mode("host=::1 user=u sslmode=disable").unwrap(), SslMode::Disable);
    }

    /// Attack: a man in the middle answers "no TLS" to the SSLRequest and then asks for
    /// the password in clear. The client must abort before sending any credentials.
    #[tokio::test]
    async fn tls_strip_mitm_gets_no_password() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let mitm = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut got = Vec::new();
            let mut req = [0u8; 8];
            if s.read_exact(&mut req).await.is_ok() {
                got.extend_from_slice(&req);
                let _ = s.write_all(b"N").await; // "this server has no TLS"
                // Startup message would follow; ask for a cleartext password.
                let mut buf = [0u8; 4096];
                if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_secs(2), s.read(&mut buf)).await {
                    got.extend_from_slice(&buf[..n]);
                    let _ = s.write_all(&[b'R', 0, 0, 0, 8, 0, 0, 0, 3]).await;
                    if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_secs(2), s.read(&mut buf)).await {
                        got.extend_from_slice(&buf[..n]);
                    }
                }
            }
            got
        });
        // The URL names a remote host; hostaddr routes it to our fake server.
        let url = format!("host=db.example.com hostaddr=127.0.0.1 port={port} user=u password=hunter2-secret dbname=d connect_timeout=5");
        assert!(PgRemote::connect(&url).await.is_err());
        let got = mitm.await.unwrap();
        assert!(!got.windows(7).any(|w| w == b"hunter2"), "password leaked to the MITM");
        assert_eq!(got.len(), 8, "only the SSLRequest may be sent before TLS is up (no startup message in clear)");
    }

    /// Needs Postgres: `source dev/test.env && cargo test -- --include-ignored`.
    #[tokio::test]
    #[ignore]
    async fn pg_two_devices() {
        let url = std::env::var("SELFKIT_TEST_PG").expect("SELFKIT_TEST_PG");
        let remote = PgRemote::connect(&url).await.unwrap();
        remote.migrate().await.unwrap(); // idempotent
        let key = SecretKey::generate();
        let app = format!("test-{}", uuid::Uuid::now_v7());
        let mut a = Store::open_in_memory(&app, "A", &[]).unwrap();
        let mut b = Store::open_in_memory(&app, "B", &[]).unwrap();
        let t = a.put("tasks", None, &json!({"title": "pg"})).unwrap();
        for i in 0..600 {
            a.put("tasks", None, &json!({"i": i})).unwrap();
        }
        assert_eq!(sync_once(&mut a, &remote, &key).await.unwrap().pushed, 601);
        assert_eq!(sync_once(&mut b, &remote, &key).await.unwrap().pulled, 601); // >1 page
        std::thread::sleep(std::time::Duration::from_millis(20)); // both edits newer than the shared version
        a.put("tasks", Some(&t.id), &json!({"title": "pg\nA"})).unwrap(); // long text: a real conflict
        b.put("tasks", Some(&t.id), &json!({"title": "pg\nB"})).unwrap();
        sync_once(&mut a, &remote, &key).await.unwrap();
        assert_eq!(sync_once(&mut b, &remote, &key).await.unwrap().conflicts, 1);
        remote.meta_set("k", "v1").await.unwrap();
        remote.meta_set("k", "v2").await.unwrap();
        assert_eq!(remote.meta_get("k").await.unwrap().as_deref(), Some("v2"));
        assert!(remote.size_bytes().await.unwrap() > 0);
    }
}
