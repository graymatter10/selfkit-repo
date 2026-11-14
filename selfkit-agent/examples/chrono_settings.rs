//! Dev/test helper: merge fields into Chrono's settings record (what the Chrono
//! app writes), e.g.
//! `cargo run -p selfkit-agent --example chrono_settings -- '{"exclude_domains":["b.test"]}'`.
//! Uses SELFKIT_DATA_DIR like the agent.
use selfkit_core::{Store, paths};

fn main() -> anyhow::Result<()> {
    let patch: serde_json::Value = serde_json::from_str(&std::env::args().nth(1).unwrap_or_else(|| "{}".into()))?;
    let store = Store::open(paths::app_db("chrono"), "chrono", &paths::device_id()?, &[])?;
    let mut cur = store.get("settings")?.filter(|r| !r.deleted).map(|r| r.data).unwrap_or_else(|| serde_json::json!({}));
    for (k, v) in patch.as_object().into_iter().flatten() {
        cur[k] = v.clone();
    }
    store.put("chrono_settings", Some("settings"), &cur)?;
    println!("{cur}");
    Ok(())
}
