//! Postgres [`Remote`] — standard SQL only, works on any Postgres ≥ 12
//! (Neon, Aiven, self-hosted). TLS via rustls when the server offers it /
//! `sslmode=require` is set.

use std::sync::Arc;

use tokio_postgres::{Client, NoTls};

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

impl PgRemote {
    /// Connect with a standard connection string. `sslmode=disable` uses plain TCP.
    pub async fn connect(url: &str) -> Result<Self> {
        let client = if url.contains("sslmode=disable") {
            let (c, conn) = tokio_postgres::connect(url, NoTls).await.map_err(err)?;
            tokio::spawn(async move {
                if let Err(e) = conn.await {
                    tracing::warn!("postgres connection: {e}");
                }
            });
            c
        } else {
            let (c, conn) = tokio_postgres::connect(url, tls()).await.map_err(err)?;
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
                     ON CONFLICT (id) DO NOTHING RETURNING rev",
                    &[&rec.id, &rec.app, &rec.collection, &rec.updated_at, &rec.device_id, &rec.ciphertext],
                )
                .await
        } else {
            self.client
                .query_opt(
                    "UPDATE sk_records SET app=$2, collection=$3, updated_at=$4, device_id=$5, ciphertext=$6,
                       rev=nextval('sk_rev_seq')
                     WHERE id=$1 AND rev=$7 RETURNING rev",
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

    async fn fetch(&self, id: &str) -> Result<Option<RemoteRecord>> {
        let r = self
            .client
            .query_opt(
                "SELECT id, app, collection, updated_at, device_id, ciphertext, rev FROM sk_records WHERE id=$1",
                &[&id],
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
        a.put("tasks", Some(&t.id), &json!({"title": "A"})).unwrap();
        b.put("tasks", Some(&t.id), &json!({"title": "B"})).unwrap();
        sync_once(&mut a, &remote, &key).await.unwrap();
        assert_eq!(sync_once(&mut b, &remote, &key).await.unwrap().conflicts, 1);
        remote.meta_set("k", "v1").await.unwrap();
        remote.meta_set("k", "v2").await.unwrap();
        assert_eq!(remote.meta_get("k").await.unwrap().as_deref(), Some("v2"));
        assert!(remote.size_bytes().await.unwrap() > 0);
    }
}
