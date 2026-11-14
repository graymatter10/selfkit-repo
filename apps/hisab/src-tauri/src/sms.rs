//! Bank / mobile-wallet SMS → review queue. Android reads SMS only after the
//! user turns the import on (READ_SMS + RECEIVE_SMS, asked at that moment);
//! messages are parsed here and stored as `sms` records with status
//! `pending`. Nothing becomes a transaction until the user taps Save in the
//! review list. Non-money SMS (OTP, chat) are ignored.

use std::sync::LazyLock;

use regex::Regex;
use selfkit_core::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{self, R, SMS, s};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parsed {
    pub kind: String,
    pub amount: f64,
    pub currency: String,
    pub merchant: String,
    pub hint: String,
}

static AMT_BEFORE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(tk\.?|bdt|usd|inr|rs\.?|pkr|eur|gbp|npr|lkr|\$|৳|€|£)\s*([0-9][0-9,]*(?:\.[0-9]{1,2})?)").unwrap()
});
static AMT_AFTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b([0-9][0-9,]*(?:\.[0-9]{1,2})?)\s*(tk|taka|bdt|usd|inr|pkr|eur|gbp|npr|lkr)\b").unwrap());
static MERCHANT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:at|to|from|merchant:?|sender:?|receiver:?)\s+([A-Za-z0-9][A-Za-z0-9 .&@'_/-]{1,40}?)(?:\s+(?:on|is|was|has|via|ref|for|with|successful|trxid|txnid)\b|[.,;:]\s|[.,;]$|\s*\(|\s*$)").unwrap()
});
static CARD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(?:a/c|acct?|account|card)[^0-9]{0,12}(?:x+|\*+)?([0-9]{3,4})\b").unwrap());

fn currency_of(tok: &str) -> &'static str {
    match tok.trim_end_matches('.').to_lowercase().as_str() {
        "tk" | "taka" | "bdt" | "৳" => "BDT",
        "$" | "usd" => "USD",
        "rs" | "inr" => "INR",
        "pkr" => "PKR",
        "eur" | "€" => "EUR",
        "gbp" | "£" => "GBP",
        "npr" => "NPR",
        "lkr" => "LKR",
        _ => "",
    }
}

fn first_pos(hay: &str, words: &[&str]) -> Option<usize> {
    words.iter().filter_map(|w| hay.find(w)).min()
}

pub fn parse(sender: &str, body: &str) -> Option<Parsed> {
    let low = body.to_lowercase();
    // Amount: first currency amount that is not a balance, fee or limit.
    let mut cands: Vec<(usize, f64, &'static str)> = Vec::new();
    for c in AMT_BEFORE.captures_iter(body) {
        let m = c.get(0).unwrap();
        if let Ok(a) = c[2].replace(',', "").parse() {
            cands.push((m.start(), a, currency_of(&c[1])));
        }
    }
    for c in AMT_AFTER.captures_iter(body) {
        let m = c.get(0).unwrap();
        if let Ok(a) = c[1].replace(',', "").parse() {
            cands.push((m.start(), a, currency_of(&c[2])));
        }
    }
    cands.sort_by_key(|c| c.0);
    let (_, amount, cur) = cands.into_iter().find(|(pos, amt, _)| {
        let pre: Vec<char> = body[..*pos].to_lowercase().chars().collect();
        let before: String = pre[pre.len().saturating_sub(25)..].iter().collect();
        *amt > 0.0 && !["balance", "bal ", "bal:", "bal.", "fee", "limit", "charge"].iter().any(|w| before.contains(w))
    })?;
    let income = first_pos(&low, &["credited", "received", "cash in", "deposited", "refund", "salary"]);
    let strong = first_pos(&low, &["debited", "spent", "cash out", "withdrawn", "withdrawal", "purchase", "send money", "sent ", "paid"]);
    let weak = first_pos(&low, &["payment", "bill", "debit", "transfer"]);
    let kind = match (income, strong, weak) {
        (Some(i), Some(e), _) => {
            if i < e { "income" } else { "expense" }
        }
        (Some(_), None, _) => "income",
        (None, Some(_), _) | (None, None, Some(_)) => "expense",
        _ => return None,
    };
    let merchant = MERCHANT
        .captures_iter(body)
        .map(|c| c[1].trim().to_string())
        .find(|m| {
            let ml = m.to_lowercase();
            !(m.contains('/') && m.chars().next().is_some_and(|c| c.is_ascii_digit())) && !ml.starts_with("a/c") && !ml.starts_with("your") && !ml.starts_with("card")
        })
        .unwrap_or_default();
    let mut hint = sender.trim().to_string();
    if let Some(c) = CARD.captures(body) {
        hint = format!("{hint} ··{}", &c[1]).trim().to_string();
    }
    Some(Parsed { kind: kind.into(), amount: model::round2(amount), currency: cur.into(), merchant, hint })
}

#[derive(Debug, Deserialize)]
pub struct Incoming {
    pub sender: String,
    pub body: String,
    /// UTC ms the SMS arrived.
    pub at: i64,
}

/// Parse and queue new messages; skips non-money SMS and ones already queued.
/// Returns how many were added.
pub fn ingest(store: &Store, msgs: &[Incoming]) -> R<usize> {
    let existing = model::all(store, SMS)?;
    let mut keys: std::collections::HashSet<String> = existing.iter().map(|v| s(v, "key").to_string()).collect();
    let mut n = 0;
    for m in msgs {
        let key = format!("{}|{}|{}", m.sender, m.at / 1000, m.body.len());
        if keys.contains(&key) {
            continue;
        }
        let Some(p) = parse(&m.sender, &m.body) else { continue };
        store
            .put(SMS, None, &json!({"key": key, "sender": m.sender, "body": m.body, "at": m.at, "status": "pending", "parsed": p}))
            .map_err(|e| e.to_string())?;
        keys.insert(key);
        n += 1;
    }
    Ok(n)
}

pub fn pending(store: &Store) -> R<Vec<Value>> {
    let mut v: Vec<Value> = model::all(store, SMS)?.into_iter().filter(|x| s(x, "status") == "pending").collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.get("at").and_then(Value::as_i64).unwrap_or(0)));
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(sender: &str, body: &str) -> Parsed {
        parse(sender, body).unwrap_or_else(|| panic!("no parse: {body}"))
    }

    #[test]
    fn wallet_and_bank_formats() {
        let a = p("bKash", "You have received Tk 1,500.00 from 01711234567. Fee Tk 0.00. Balance Tk 3,250.50. TrxID 9KL4ABCD at 15/11/2026 10:22");
        assert_eq!((a.kind.as_str(), a.amount, a.currency.as_str(), a.merchant.as_str()), ("income", 1500.0, "BDT", "01711234567"));
        let b = p("bKash", "Payment of Tk 450.00 to Daraz Bangladesh is successful. Balance Tk 2,800.50. TrxID 9KL4XYZ");
        assert_eq!((b.kind.as_str(), b.amount, b.merchant.as_str()), ("expense", 450.0, "Daraz Bangladesh"));
        let c = p("DBBL", "Your A/C XXXX1234 has been debited by BDT 2,350.00 on 14-Nov-2026 at SHWAPNO GULSHAN. Available balance BDT 45,120.00");
        assert_eq!((c.kind.as_str(), c.amount, c.merchant.as_str(), c.hint.as_str()), ("expense", 2350.0, "SHWAPNO GULSHAN", "DBBL ··1234"));
        let d = p("CITI", "USD 25.00 spent on card ending 4321 at AMAZON.COM on 15-11-2026.");
        assert_eq!((d.kind.as_str(), d.amount, d.currency.as_str(), d.merchant.as_str()), ("expense", 25.0, "USD", "AMAZON.COM"));
        let e = p("NAGAD", "Cash In Received. Amount: Tk 2000.00 Sender: 01711000000 TxnID: 7AB Balance: Tk 2500.00");
        assert_eq!((e.kind.as_str(), e.amount), ("income", 2000.0));
        let f0 = p("HDFC", "INR 499.00 debited from A/c XX5678 on 15-11-26 to VPA netflix@icici. Avl Bal INR 12,000");
        assert_eq!((f0.kind.as_str(), f0.amount, f0.currency.as_str()), ("expense", 499.0, "INR"));
        let g = p("bKash", "Cash Out Tk 1,000.00 to 01812345678 successful. Fee Tk 18.50. Balance Tk 1,781.50.");
        assert_eq!((g.kind.as_str(), g.amount, g.merchant.as_str()), ("expense", 1000.0, "01812345678"));
    }

    #[test]
    fn ignores_non_money() {
        assert!(parse("Google", "G-482913 is your Google verification code.").is_none());
        assert!(parse("Mom", "Come home at 8, dinner is ready").is_none());
        assert!(parse("bKash", "Your OTP is 482913. Do not share.").is_none());
    }

    #[test]
    fn ingest_dedupes() {
        let st = Store::open_in_memory("hisab", "d", &[]).unwrap();
        let m = vec![
            Incoming { sender: "bKash".into(), body: "Payment of Tk 450.00 to Daraz is successful.".into(), at: 1_700_000_000_000 },
            Incoming { sender: "Mom".into(), body: "hi".into(), at: 1_700_000_000_000 },
        ];
        assert_eq!(ingest(&st, &m).unwrap(), 1);
        assert_eq!(ingest(&st, &m).unwrap(), 0);
        assert_eq!(pending(&st).unwrap().len(), 1);
    }
}
