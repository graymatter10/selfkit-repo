//! Main currency (shared Selfkit setting `main_currency`) as Hisab uses it.
//!
//! - `read_main` reads only the `main_currency` field of the shared settings
//!   record, so a record that core's `Settings` can't fully parse (a field
//!   written by a newer app version) no longer silently means "USD".
//! - `set_main` changes only that field (other apps' fields are kept) and
//!   moves accounts that still follow the old default onto the new one.
//! - `relabel_account` is the user-confirmed fix for accounts that were used
//!   while they carried the wrong default: same numbers, new currency label.
//!
//! Docs: docs/apps/hisab/currencies.md.

use selfkit_core::Store;
use selfkit_core::settings::{COLLECTION, SETTINGS_ID};
use serde_json::{Value, json};

use crate::model::{self, ACCOUNTS, LOANS, R, RECURRING, TRANSACTIONS, f, s};

fn valid(code: &str) -> Option<String> {
    let c = code.trim().to_ascii_uppercase();
    (c.len() == 3 && c.chars().all(|ch| ch.is_ascii_alphabetic())).then_some(c)
}

/// The stored main currency, or None when there is no (readable) setting.
pub fn read_main(settings: &Store) -> Option<String> {
    let r = settings.get(SETTINGS_ID).ok()??;
    if r.deleted {
        return None;
    }
    r.data.get("main_currency").and_then(Value::as_str).and_then(valid)
}

/// Result of a main-currency change.
#[derive(Debug, serde::Serialize)]
pub struct MainChange {
    pub old: String,
    pub new: String,
    /// Unused accounts in the old main currency, switched automatically.
    pub switched: Vec<String>,
    /// Used accounts in the old main currency: `{id, name, currency, entries}`.
    /// The UI offers to relabel them (never done without asking).
    pub used: Vec<Value>,
}

/// Entries (transactions, loans, recurring items) that point at an account.
fn uses(store: &Store, id: &str) -> R<usize> {
    let n = model::all(store, TRANSACTIONS)?.iter().filter(|t| s(t, "account") == id || s(t, "to_account") == id).count()
        + model::all(store, LOANS)?.iter().filter(|l| s(l, "account") == id).count()
        + model::all(store, RECURRING)?
            .iter()
            .filter(|r| r.get("template").map(|t| s(t, "account") == id).unwrap_or(false))
            .count();
    Ok(n)
}

/// Set the main currency to `code`. `fallback_old` is the last known main
/// (used when nothing is stored yet). Accounts in the old main currency with
/// no entries and no opening balance followed the default (e.g. the "Cash"
/// account made at first start) and switch to the new main currency.
pub fn set_main(settings: &Store, store: &Store, code: &str, fallback_old: &str) -> R<MainChange> {
    let new = valid(code).ok_or_else(|| format!("Unknown currency “{code}”"))?;
    let old = read_main(settings).unwrap_or_else(|| fallback_old.to_string());
    // Change only main_currency; keep every other field exactly as stored.
    let mut data = settings
        .get(SETTINGS_ID)
        .map_err(|e| e.to_string())?
        .filter(|r| !r.deleted && r.data.is_object())
        .map(|r| r.data)
        .unwrap_or_else(|| serde_json::to_value(selfkit_core::settings::Settings::default()).unwrap_or_else(|_| json!({})));
    data["main_currency"] = json!(new);
    settings.put(COLLECTION, Some(SETTINGS_ID), &data).map_err(|e| e.to_string())?;

    let mut out = MainChange { old: old.clone(), new: new.clone(), switched: vec![], used: vec![] };
    if old == new {
        return Ok(out);
    }
    for a in model::all(store, ACCOUNTS)? {
        if s(&a, "currency") != old {
            continue;
        }
        let id = s(&a, "id").to_string();
        let n = uses(store, &id)?;
        if n == 0 && f(&a, "opening") == 0.0 {
            put_currency(store, ACCOUNTS, &a, &new)?;
            out.switched.push(id);
        } else if !model::b(&a, "archived") {
            out.used.push(json!({"id": id, "name": s(&a, "name"), "currency": old, "entries": n}));
        }
    }
    Ok(out)
}

fn put_currency(store: &Store, coll: &str, v: &Value, cur: &str) -> R<()> {
    let id = s(v, "id").to_string();
    let mut d = v.clone();
    if let Some(m) = d.as_object_mut() {
        m.remove("id");
        m.insert("currency".into(), json!(cur));
    }
    store.put(coll, Some(&id), &d).map_err(|e| e.to_string())?;
    Ok(())
}

/// Relabel an account (asked the user first): its currency becomes `to`, and
/// its own entries that were in the account's old currency get the same label.
/// Numbers are not converted. Returns the number of entries changed.
pub fn relabel_account(store: &Store, id: &str, to: &str) -> R<usize> {
    let to = valid(to).ok_or_else(|| format!("Unknown currency “{to}”"))?;
    let a = model::get(store, id)?.ok_or("Account not found")?;
    let from = s(&a, "currency").to_string();
    if from == to {
        return Ok(0);
    }
    put_currency(store, ACCOUNTS, &a, &to)?;
    let mut n = 0;
    for t in model::all(store, TRANSACTIONS)? {
        if s(&t, "account") == id && s(&t, "currency") == from {
            put_currency(store, TRANSACTIONS, &t, &to)?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{normalize, seed_defaults};
    use selfkit_core::settings::{APP, Settings};

    fn stores() -> (Store, Store) {
        (Store::open_in_memory(APP, "d", &[]).unwrap(), Store::open_in_memory("hisab", "d", model::MIGRATIONS).unwrap())
    }
    fn cash(st: &Store) -> Value {
        model::all(st, ACCOUNTS).unwrap().into_iter().find(|a| s(a, "name") == "Cash").unwrap()
    }

    /// The bug: first start seeds "Cash" in the default USD; choosing BDT later
    /// left Cash (and so the add screen, which follows the account) on USD,
    /// and after every save / restart the add screen went back to USD.
    #[test]
    fn main_change_moves_default_cash_account() {
        let (set, st) = stores();
        seed_defaults(&st, read_main(&set).as_deref().unwrap_or("USD")).unwrap();
        assert_eq!(s(&cash(&st), "currency"), "USD");
        let ch = set_main(&set, &st, "bdt", "USD").unwrap();
        assert_eq!((ch.old.as_str(), ch.new.as_str(), ch.switched.len()), ("USD", "BDT", 1));
        // "Restart": everything read again from the stores.
        assert_eq!(read_main(&set).as_deref(), Some("BDT"));
        assert_eq!(Settings::load(&set).unwrap().main_currency, "BDT");
        assert_eq!(s(&cash(&st), "currency"), "BDT");
        let t = normalize(TRANSACTIONS, &json!({"kind":"expense","amount":5,"date":"2026-11-15","account":"a","category":"c"}), &read_main(&set).unwrap()).unwrap();
        assert_eq!(s(&t, "currency"), "BDT");
    }

    #[test]
    fn used_accounts_are_offered_not_changed() {
        let (set, st) = stores();
        seed_defaults(&st, "USD").unwrap();
        let id = s(&cash(&st), "id").to_string();
        let eur = st.put(ACCOUNTS, None, &json!({"name":"Euro","currency":"EUR","opening":0})).unwrap().id;
        st.put(TRANSACTIONS, None, &json!({"kind":"expense","amount":120,"currency":"USD","account":id,"date":"2026-11-15"})).unwrap();
        st.put(TRANSACTIONS, None, &json!({"kind":"expense","amount":3,"currency":"EUR","account":id,"date":"2026-11-15"})).unwrap();
        let ch = set_main(&set, &st, "BDT", "USD").unwrap();
        assert!(ch.switched.is_empty(), "an account with entries keeps its currency");
        assert_eq!(ch.used.len(), 1);
        assert_eq!(s(&model::get(&st, &eur).unwrap().unwrap(), "currency"), "EUR", "explicit EUR untouched");
        assert_eq!(relabel_account(&st, &id, "BDT").unwrap(), 1, "only the USD entry is relabelled");
        assert_eq!(s(&cash(&st), "currency"), "BDT");
        let txs = model::all(&st, TRANSACTIONS).unwrap();
        assert!(txs.iter().any(|t| s(t, "currency") == "BDT" && f(t, "amount") == 120.0));
        assert!(txs.iter().any(|t| s(t, "currency") == "EUR"));
    }

    /// A settings record core can't fully parse used to read as "USD".
    #[test]
    fn odd_settings_record_keeps_main_and_other_fields() {
        let (set, st) = stores();
        set.put(COLLECTION, Some(SETTINGS_ID), &json!({"main_currency":"BDT","theme":{"mode":"future"},"extra":1})).unwrap();
        assert_eq!(Settings::load(&set).unwrap().main_currency, "USD", "core's full parse falls back to the default");
        assert_eq!(read_main(&set).as_deref(), Some("BDT"));
        set_main(&set, &st, "EUR", "USD").unwrap();
        let d = set.get(SETTINGS_ID).unwrap().unwrap().data;
        assert_eq!((d["main_currency"].as_str(), d["extra"].as_i64()), (Some("EUR"), Some(1)));
        assert!(set_main(&set, &st, "12", "USD").is_err());
    }
}
