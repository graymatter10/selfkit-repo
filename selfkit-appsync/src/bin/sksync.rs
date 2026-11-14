//! `sksync`: a headless "device" for sync tests (dev/sync-e2e.sh, CI). One data
//! folder = one simulated device running one app. Prints JSON.
//!
//! sksync --dir D --app A [--name N] [--test-key | --key B64] [--pg URL] [--s3 JSON]
//!        [--mode auto|lan|server] [--peer ip:port]... [--no-discovery] <command> [args]
//! Commands: put COLL JSON [ID] · get ID · delete ID · restore ID · count COLL ·
//!   bulk COLL N [PREFIX] · sync · status · conflicts · resolve STORE ID a|b|merge:JSON ·
//!   dump · photo-add FILE · photo-hash ID · settings

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::Arc;

use selfkit_appsync::secrets::{self, FileSecrets, SecretStore};
use selfkit_appsync::{AppSync, AppSyncConfig, Choice, SyncOwner, Trigger, TransportMode};
use serde_json::{Value, json};
use sha2::Digest;

fn die(msg: &str) -> ExitCode {
    eprintln!("sksync: {msg}");
    ExitCode::from(1)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut opt = BTreeMap::<String, Vec<String>>::new();
    let mut flags = std::collections::BTreeSet::<String>::new();
    let mut rest = vec![];
    while !args.is_empty() {
        let a = args.remove(0);
        match a.as_str() {
            "--test-key" | "--no-discovery" => {
                flags.insert(a);
            }
            s if s.starts_with("--") && !args.is_empty() => {
                opt.entry(a).or_default().push(args.remove(0));
            }
            _ => rest.push(a),
        }
    }
    let one = |k: &str| opt.get(k).and_then(|v| v.last().cloned());
    let (Some(dir), Some(app)) = (one("--dir"), one("--app")) else { return die("need --dir and --app") };
    let secrets: Arc<dyn SecretStore> = Arc::new(FileSecrets::new(std::path::Path::new(&dir).join("sync/secrets.json"), None));
    if let Some(k) = one("--key") {
        secrets.set(secrets::DATA_KEY, &k).unwrap();
    }
    let mut cfg = AppSyncConfig::new(&app, &dir, secrets);
    cfg.test_key = flags.contains("--test-key");
    cfg.default_owner = SyncOwner::App;
    if let Some(n) = one("--name") {
        cfg.device_name = n;
    }
    let sync = AppSync::new(cfg).unwrap();
    if let Some(pg) = one("--pg") {
        let s3 = one("--s3").map(|j| serde_json::from_str(&j).expect("--s3 json"));
        sync.set_server(Some(&selfkit_appsync::ServerConfig { pg_url: pg, s3 })).unwrap();
    }
    let mut st = sync.settings();
    st.owner = SyncOwner::App;
    if let Some(m) = one("--mode") {
        st.transport = match m.as_str() {
            "lan" => TransportMode::LanOnly,
            "server" => TransportMode::ServerOnly,
            _ => TransportMode::Auto,
        };
    }
    if let Some(p) = opt.get("--peer") {
        st.lan_peers = p.clone();
    }
    if flags.contains("--no-discovery") {
        st.lan_discovery_ms = 0;
    }
    if let Some(p) = one("--discovery-port") {
        st.lan_discovery_port = p.parse().unwrap();
    }
    sync.set_settings(&st).unwrap();

    let Some(cmd) = rest.first().cloned() else { return die("no command") };
    let arg = |i: usize| rest.get(i).cloned().unwrap_or_default();
    let stores = sync.open_stores().unwrap();
    let store = &stores[&app];
    let out: Value = match cmd.as_str() {
        "put" => {
            let data: Value = serde_json::from_str(&arg(2)).expect("json");
            let id = rest.get(3).map(|s| s.as_str());
            json!(store.put(&arg(1), id, &data).unwrap().id)
        }
        "get" => serde_json::to_value(store.get(&arg(1)).unwrap()).unwrap(),
        "delete" => {
            store.delete(&arg(1)).unwrap();
            json!("ok")
        }
        "restore" => {
            store.restore(&arg(1)).unwrap();
            json!("ok")
        }
        "count" => json!(store.count(&arg(1)).unwrap()),
        "bulk" => {
            let n: usize = arg(2).parse().unwrap();
            let prefix = rest.get(3).cloned().unwrap_or_else(|| "item".into());
            let tx = store.conn().unchecked_transaction().unwrap();
            for i in 0..n {
                store
                    .put(&arg(1), None, &json!({"kind": "expense", "amount": (i % 997) as f64 + 0.5, "currency": "BDT", "date": "2026-11-14", "note": format!("{prefix} {i}"), "source": "import", "tags": [], "splits": [], "photos": []}))
                    .unwrap();
            }
            tx.commit().unwrap();
            json!(n)
        }
        "sync" => {
            drop(stores);
            let o = sync.sync_now(Trigger::Manual).await;
            println!("{}", serde_json::to_string(&o).unwrap());
            return if o.ok && o.ran { ExitCode::SUCCESS } else { ExitCode::from(2) };
        }
        "status" => serde_json::to_value(sync.status().await.unwrap()).unwrap(),
        "conflicts" => serde_json::to_value(sync.conflicts().unwrap()).unwrap(),
        "resolve" => {
            let c = arg(3);
            let choice = match c.as_str() {
                "a" => Choice::KeepA,
                "b" => Choice::KeepB,
                m => serde_json::from_str(m.strip_prefix("merge:").expect("a|b|merge:JSON")).expect("merge json"),
            };
            sync.resolve(&arg(1), &arg(2), &choice).unwrap();
            json!("ok")
        }
        "dump" => {
            // Content-only view for convergence checks (no device/time fields).
            let mut all = vec![];
            for (name, s) in &stores {
                let mut st = s.conn().prepare("SELECT id, collection, data, deleted FROM records ORDER BY id").unwrap();
                let rows: Vec<Value> = st
                    .query_map([], |r| {
                        Ok(json!({"store": name, "id": r.get::<_, String>(0)?, "c": r.get::<_, String>(1)?,
                            "data": serde_json::from_str::<Value>(&r.get::<_, String>(2)?).unwrap_or(Value::Null), "del": r.get::<_, i64>(3)?}))
                    })
                    .unwrap()
                    .map(|r| r.unwrap())
                    .collect();
                all.extend(rows);
            }
            all.sort_by(|a, b| (a["store"].as_str(), a["id"].as_str()).cmp(&(b["store"].as_str(), b["id"].as_str())));
            json!(all)
        }
        "photo-add" => {
            let src = arg(1);
            let bytes = std::fs::read(&src).unwrap();
            let id = selfkit_core::store::new_id();
            let file = format!("{id}.webp");
            let d = std::path::Path::new(&dir).join(format!("{app}-photos"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(&file), &bytes).unwrap();
            let sha = format!("{:x}", sha2::Sha256::digest(&bytes));
            store.put("photos", Some(&id), &json!({"file": file, "mime": "image/webp", "bytes": bytes.len(), "sha256": sha, "created_at": selfkit_core::time::now_ms()})).unwrap();
            json!(id)
        }
        "photo-hash" => {
            let r = store.get(&arg(1)).unwrap().expect("photo record");
            let p = std::path::Path::new(&dir).join(format!("{app}-photos")).join(r.data["file"].as_str().unwrap());
            match std::fs::read(p) {
                Ok(b) => json!(format!("{:x}", sha2::Sha256::digest(&b))),
                Err(_) => json!("missing"),
            }
        }
        "settings" => serde_json::to_value(sync.settings()).unwrap(),
        c => return die(&format!("unknown command {c}")),
    };
    println!("{out}");
    ExitCode::SUCCESS
}
