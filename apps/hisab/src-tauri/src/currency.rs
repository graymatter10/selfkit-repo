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
use selfkit_core::settings::{SETTINGS_ID, Settings};
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

/// The currency every install starts with when nothing was ever chosen.
fn first_default() -> String {
    Settings::default().main_currency
}

/// Was this account's currency chosen by the user? New accounts store
/// `explicit_currency` (set when the currency is picked in the account sheet,
/// or "Keep" on the relabel offer). Older accounts without the flag count as
/// chosen unless their currency is one of the `defaults` they could have got
/// without asking (the first-start USD, or the main currency of the time).
pub fn explicit(a: &Value, defaults: &[&str]) -> bool {
    match a.get("explicit_currency").and_then(Value::as_bool) {
        Some(x) => x,
        None => !defaults.contains(&s(a, "currency")),
    }
}

/// Currency new entries on this account start in: the account's own when the
/// user chose it, otherwise the main currency ("main everywhere unless told
/// otherwise"). Entries in another currency still convert for the balance.
pub fn entry_currency(a: &Value, main: &str) -> String {
    if explicit(a, &[&first_default(), main]) { s(a, "currency").to_string() } else { main.to_string() }
}

/// Accounts in another currency than `main` that the user never chose
/// (e.g. "Cash" seeded in USD before the main currency was set). Unused ones
/// are switched to `main` (returned in `.0`); used ones are returned in `.1`
/// as `{id, name, currency, entries}` for a relabel offer.
fn not_following(store: &Store, main: &str, defaults: &[&str], switch_unused: bool) -> R<(Vec<String>, Vec<Value>)> {
    let (mut switched, mut used) = (vec![], vec![]);
    for a in model::all(store, ACCOUNTS)? {
        let cur = s(&a, "currency").to_string();
        if cur == main || model::b(&a, "archived") || explicit(&a, defaults) {
            continue;
        }
        let id = s(&a, "id").to_string();
        let n = uses(store, &id)?;
        if switch_unused && n == 0 && f(&a, "opening") == 0.0 {
            put_currency(store, ACCOUNTS, &a, main)?;
            switched.push(id);
        } else {
            used.push(json!({"id": id, "name": s(&a, "name"), "currency": cur, "entries": n}));
        }
    }
    Ok((switched, used))
}

/// At startup: unused default accounts follow the main currency.
pub fn follow_main(store: &Store, main: &str) -> R<Vec<String>> {
    Ok(not_following(store, main, &[&first_default()], true)?.0)
}

/// Used accounts still in a currency the user never chose, for the gentle
/// "Cash is in USD — switch to BDT?" offer (Settings, Accounts, Home).
pub fn stale_accounts(store: &Store, main: &str) -> R<Vec<Value>> {
    Ok(not_following(store, main, &[&first_default()], false)?.1)
}

/// Set the main currency to `code`. `fallback_old` is the last known main
/// (used when nothing is stored yet). Accounts whose currency the user never
/// chose (the first-start default or the old main) follow it: unused ones
/// switch, used ones are offered (also when the same currency is re-picked,
/// e.g. BDT was set before this fix but "Cash" was seeded in USD and used).
pub fn set_main(settings: &Store, store: &Store, code: &str, fallback_old: &str) -> R<MainChange> {
    let new = valid(code).ok_or_else(|| format!("Unknown currency “{code}”"))?;
    let old = read_main(settings).unwrap_or_else(|| fallback_old.to_string());
    // Change only main_currency; keep every other field exactly as stored.
    Settings::update(settings, |x| x.main_currency = new.clone()).map_err(|e| e.to_string())?;
    if old != new {
        convert_budgets(store, &old, &new)?;
    }
    let (switched, used) = not_following(store, &new, &[&first_default(), &old], true)?;
    Ok(MainChange { old, new, switched, used })
}

/// Category budgets are stored in the main currency: convert them with the
/// saved rates (left as they are when a rate is missing).
fn convert_budgets(store: &Store, old: &str, new: &str) -> R<()> {
    let fx = crate::fx::Fx::load(store)?;
    let probe = fx.conv(1.0, old, new);
    if !fx.missing.borrow().is_empty() || probe <= 0.0 {
        return Ok(());
    }
    for c in model::all(store, model::CATEGORIES)? {
        let b = f(&c, "budget");
        if b > 0.0 {
            let id = s(&c, "id").to_string();
            let mut d = c.clone();
            if let Some(m) = d.as_object_mut() {
                m.remove("id");
                m.insert("budget".into(), json!(model::round2(b * probe)));
            }
            store.put(model::CATEGORIES, Some(&id), &d).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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
    // Answering the offer with "Switch" is a choice too, like "Keep": never offered again
    // (before, the next main-currency change offered to relabel the new entries back, L2-27).
    let mut acct = a.clone();
    if let Some(m) = acct.as_object_mut() {
        m.insert("explicit_currency".into(), json!(true));
    }
    put_currency(store, ACCOUNTS, &acct, &to)?;
    let mut n = 0;
    for t in model::all(store, TRANSACTIONS)? {
        if s(&t, "account") == id && s(&t, "currency") == from {
            put_currency(store, TRANSACTIONS, &t, &to)?;
            n += 1;
        }
    }
    Ok(n)
}

/// "Keep USD" on the relabel offer: the account's currency now counts as
/// chosen by the user (never offered again; new entries start in it).
pub fn keep_currency(store: &Store, id: &str) -> R<()> {
    let a = model::get(store, id)?.ok_or("Account not found")?;
    let mut d = a.clone();
    if let Some(m) = d.as_object_mut() {
        m.remove("id");
        m.insert("explicit_currency".into(), json!(true));
    }
    store.put(ACCOUNTS, Some(id), &d).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{normalize, seed_defaults};
    use selfkit_core::settings::{APP, COLLECTION};

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
    fn budgets_follow_the_main_currency() {
        let (set, st) = stores();
        crate::fx::save_fetched(&st, "USD", &json!({"BDT": 120.0})).unwrap();
        let food = st.put(model::CATEGORIES, None, &json!({"name":"Food","kind":"expense","budget":100})).unwrap().id;
        set_main(&set, &st, "BDT", "USD").unwrap();
        assert_eq!(f(&model::get(&st, &food).unwrap().unwrap(), "budget"), 12000.0);
        set_main(&set, &st, "XYZ", "USD").unwrap(); // no rate: kept
        assert_eq!(f(&model::get(&st, &food).unwrap().unwrap(), "budget"), 12000.0);
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

    /// Main was already BDT before this fix, but Cash was seeded in USD and
    /// used: picking BDT again offers it; an unused USD default follows main.
    #[test]
    fn repick_same_offers_default_currency_accounts() {
        let (set, st) = stores();
        seed_defaults(&st, "USD").unwrap();
        set_main(&set, &st, "BDT", "USD").unwrap(); // unused Cash switches
        let usd = st.put(ACCOUNTS, None, &json!({"name":"Old","currency":"USD","opening":0})).unwrap().id;
        let empty = st.put(ACCOUNTS, None, &json!({"name":"Empty","currency":"USD","opening":0})).unwrap().id;
        st.put(TRANSACTIONS, None, &json!({"kind":"expense","amount":5,"currency":"USD","account":usd,"date":"2026-11-15"})).unwrap();
        let ch = set_main(&set, &st, "BDT", "USD").unwrap();
        assert_eq!(ch.switched, vec![empty.clone()]);
        let ids: Vec<&str> = ch.used.iter().map(|u| s(u, "id")).collect();
        assert_eq!(ids, vec![usd.as_str()]);
        assert_eq!(s(&model::get(&st, &usd).unwrap().unwrap(), "currency"), "USD", "used: not changed without asking");
    }

    /// The phone case: main has been BDT for a while, "Cash" is USD with
    /// entries. The offer is there without re-picking anything (snapshot),
    /// new entries default to BDT, and "Keep USD" makes it a chosen currency.
    #[test]
    fn stale_default_account_is_offered_and_entries_use_main() {
        let (set, st) = stores();
        seed_defaults(&st, "USD").unwrap();
        let id = s(&cash(&st), "id").to_string();
        st.put(TRANSACTIONS, None, &json!({"kind":"expense","amount":2,"currency":"USD","account":id,"date":"2026-11-15"})).unwrap();
        set.put(COLLECTION, Some(SETTINGS_ID), &json!({"main_currency":"BDT"})).unwrap();
        let main = read_main(&set).unwrap();
        assert!(follow_main(&st, &main).unwrap().is_empty(), "used: never switched by itself");
        let stale = stale_accounts(&st, &main).unwrap();
        assert_eq!((stale.len(), s(&stale[0], "id"), stale[0]["entries"].as_u64()), (1, id.as_str(), Some(1)));
        assert_eq!(entry_currency(&cash(&st), &main), "BDT", "add screen starts in the main currency");
        // An explicitly chosen EUR account is neither offered nor overridden.
        let eur = st.put(ACCOUNTS, None, &json!({"name":"Euro","currency":"EUR","explicit_currency":true})).unwrap().id;
        st.put(TRANSACTIONS, None, &json!({"kind":"expense","amount":1,"currency":"EUR","account":eur,"date":"2026-11-15"})).unwrap();
        let e = model::get(&st, &eur).unwrap().unwrap();
        assert_eq!(entry_currency(&e, &main), "EUR");
        assert_eq!(stale_accounts(&st, &main).unwrap().len(), 1);
        // Even a USD account picked on purpose keeps USD for new entries.
        keep_currency(&st, &id).unwrap();
        assert!(stale_accounts(&st, &main).unwrap().is_empty(), "Keep USD: not offered again");
        assert_eq!(entry_currency(&cash(&st), &main), "USD");
    }

    /// L2-27: "Switch to BDT" is a choice; changing the main currency back later must not
    /// offer to relabel the (now real taka) entries again.
    #[test]
    fn switch_is_never_offered_again() {
        let (set, st) = stores();
        seed_defaults(&st, "USD").unwrap();
        let id = s(&cash(&st), "id").to_string();
        st.put(TRANSACTIONS, None, &json!({"kind":"expense","amount":2,"currency":"USD","account":id,"date":"2026-11-15"})).unwrap();
        set.put(COLLECTION, Some(SETTINGS_ID), &json!({"main_currency":"BDT"})).unwrap();
        assert_eq!(stale_accounts(&st, "BDT").unwrap().len(), 1);
        assert_eq!(relabel_account(&st, &id, "BDT").unwrap(), 1);
        st.put(TRANSACTIONS, None, &json!({"kind":"expense","amount":500,"currency":"BDT","account":id,"date":"2026-11-16"})).unwrap();
        assert!(stale_accounts(&st, "USD").unwrap().is_empty(), "back to USD: Cash stays BDT, no offer");
        assert_eq!(entry_currency(&cash(&st), "USD"), "BDT");
    }

    /// An entry in BDT on a USD account still converts for the balance.
    #[test]
    fn main_currency_entry_on_usd_account_converts() {
        let (_set, st) = stores();
        crate::fx::save_fetched(&st, "USD", &json!({"BDT": 120.0})).unwrap();
        let a = st.put(ACCOUNTS, None, &json!({"name":"Cash","currency":"USD","opening":0})).unwrap().id;
        st.put(TRANSACTIONS, None, &json!({"kind":"expense","amount":240,"currency":"BDT","account":a,"date":"2026-11-15"})).unwrap();
        let d = crate::stats::Data::load(&st, "BDT").unwrap();
        let v = crate::stats::accounts_view(&d);
        assert_eq!((f(&v[0], "balance"), f(&v[0], "balance_main")), (-2.0, -240.0));
        assert_eq!(s(&v[0], "entry_currency"), "BDT");
    }

    /// Unused default accounts follow the main currency at startup; legacy
    /// accounts in a non-default currency count as chosen.
    #[test]
    fn follow_main_switches_only_unused_unchosen() {
        let (_set, st) = stores();
        seed_defaults(&st, "USD").unwrap();
        let jpy = st.put(ACCOUNTS, None, &json!({"name":"Yen","currency":"JPY","opening":0})).unwrap().id;
        let sw = follow_main(&st, "BDT").unwrap();
        assert_eq!(sw.len(), 1);
        assert_eq!(s(&cash(&st), "currency"), "BDT");
        assert_eq!(s(&model::get(&st, &jpy).unwrap().unwrap(), "currency"), "JPY");
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
