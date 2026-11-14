//! Exchange rates: one record (`rates`/`hisab-rates`) holding rates fetched
//! online (`rates`, units per 1 `base`), hand-edited overrides (`manual`, same
//! unit, win over fetched) and `fetched_at` (UTC ms). Totals convert into the
//! main currency through these rates; an unknown currency counts 1:1 and is
//! reported in `missing` so the UI can ask for a rate.

use std::collections::{BTreeSet, HashMap};

use serde_json::{Value, json};

use crate::model::{self, R, RATES, RATES_ID};
use selfkit_core::Store;

#[derive(Debug, Clone, Default)]
pub struct Fx {
    pub base: String,
    rates: HashMap<String, f64>,
    pub missing: std::cell::RefCell<BTreeSet<String>>,
}

impl Fx {
    pub fn from_value(v: &Value) -> Fx {
        let base = model::s(v, "base");
        let base = if base.is_empty() { "USD".to_string() } else { base.to_string() };
        let mut rates = HashMap::new();
        for k in ["rates", "manual"] {
            if let Some(o) = v.get(k).and_then(Value::as_object) {
                for (c, r) in o {
                    if let Some(x) = r.as_f64().filter(|x| *x > 0.0 && x.is_finite()) {
                        rates.insert(c.to_ascii_uppercase(), x);
                    }
                }
            }
        }
        rates.insert(base.clone(), 1.0);
        Fx { base, rates, missing: Default::default() }
    }

    pub fn load(store: &Store) -> R<Fx> {
        Ok(Fx::from_value(&raw(store)?))
    }

    fn rate(&self, c: &str) -> Option<f64> {
        self.rates.get(c).copied()
    }

    /// Convert `amount` from one currency to another.
    pub fn conv(&self, amount: f64, from: &str, to: &str) -> f64 {
        if from == to || amount == 0.0 {
            return amount;
        }
        match (self.rate(from), self.rate(to)) {
            (Some(a), Some(b)) => amount / a * b,
            (a, b) => {
                let mut m = self.missing.borrow_mut();
                if a.is_none() {
                    m.insert(from.to_string());
                }
                if b.is_none() {
                    m.insert(to.to_string());
                }
                amount
            }
        }
    }
}

/// The stored rates record (or an empty USD one).
pub fn raw(store: &Store) -> R<Value> {
    Ok(model::get(store, RATES_ID)?.unwrap_or_else(|| json!({"base": "USD", "rates": {}, "manual": {}, "fetched_at": 0})))
}

/// Save fetched rates (keeps manual overrides).
pub fn save_fetched(store: &Store, base: &str, rates: &Value) -> R<Value> {
    let mut cur = raw(store)?;
    cur["base"] = json!(base.to_ascii_uppercase());
    cur["rates"] = rates.clone();
    cur["fetched_at"] = json!(selfkit_core::time::now_ms());
    cur.as_object_mut().map(|o| o.remove("id"));
    store.put(RATES, Some(RATES_ID), &cur).map_err(|e| e.to_string())?;
    raw(store)
}

/// Set (or clear with `None`) a hand-edited rate: units of `currency` per 1 base.
pub fn set_manual(store: &Store, currency: &str, rate: Option<f64>) -> R<Value> {
    let mut cur = raw(store)?;
    cur.as_object_mut().map(|o| o.remove("id"));
    if !cur.get("manual").is_some_and(Value::is_object) {
        cur["manual"] = json!({});
    }
    let c = currency.to_ascii_uppercase();
    match rate {
        Some(r) if r > 0.0 => cur["manual"][&c] = json!(r),
        _ => {
            cur["manual"].as_object_mut().map(|o| o.remove(&c));
        }
    }
    store.put(RATES, Some(RATES_ID), &cur).map_err(|e| e.to_string())?;
    raw(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_cross_and_manual() {
        let fx = Fx::from_value(&json!({"base":"USD","rates":{"EUR":0.5,"BDT":100},"manual":{"BDT":120}}));
        assert_eq!(fx.conv(10.0, "USD", "EUR"), 5.0);
        assert_eq!(fx.conv(120.0, "BDT", "USD"), 1.0);
        assert!((fx.conv(1.0, "EUR", "BDT") - 240.0).abs() < 1e-9);
        assert_eq!(fx.conv(7.0, "XYZ", "USD"), 7.0);
        assert!(fx.missing.borrow().contains("XYZ"));
    }

    #[test]
    fn manual_overrides_stored() {
        let st = Store::open_in_memory("hisab", "d", &[]).unwrap();
        save_fetched(&st, "USD", &json!({"EUR": 0.9})).unwrap();
        set_manual(&st, "bdt", Some(122.0)).unwrap();
        let fx = Fx::load(&st).unwrap();
        assert_eq!(fx.conv(122.0, "BDT", "USD"), 1.0);
        save_fetched(&st, "USD", &json!({"EUR": 0.8, "BDT": 110})).unwrap();
        assert_eq!(Fx::load(&st).unwrap().conv(122.0, "BDT", "USD"), 1.0, "manual wins over fetched");
        set_manual(&st, "BDT", None).unwrap();
        assert_eq!(Fx::load(&st).unwrap().conv(110.0, "BDT", "USD"), 1.0);
    }
}
