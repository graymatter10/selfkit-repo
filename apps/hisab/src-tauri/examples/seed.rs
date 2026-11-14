//! Sample data for Hisab (testing, screenshots): ~7 months of entries,
//! four accounts (one in BDT), budgets, loans, goals, subscriptions, SMS.
//! `SELFKIT_DATA_DIR=/tmp/x cargo run -p hisab --example seed`
//! `--theme light|dark|system` only sets the theme preference.
//! `--fresh` wipes hisab.db first.

use chrono::{Datelike, Duration, Local, NaiveDate};
use hisab_lib::model::{self, ACCOUNTS, CATEGORIES, GOALS, LOANS, RECURRING, SMS, TRANSACTIONS};
use hisab_lib::{fx, recur};
use selfkit_core::{Store, paths};
use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let db = paths::app_db("hisab");
    if args.iter().any(|a| a == "--fresh") {
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", db.display()));
        }
    }
    let device = paths::device_id()?;
    let s = Store::open(&db, "hisab", &device, model::MIGRATIONS)?;
    if let Some(i) = args.iter().position(|a| a == "--theme") {
        let mut p: Value = s.kv_get("hisab.prefs")?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_else(|| json!({}));
        p["theme"] = json!(args[i + 1]);
        s.kv_set("hisab.prefs", &p.to_string())?;
        return Ok(());
    }
    if s.count(TRANSACTIONS)? > 0 {
        println!("already seeded ({}); use --fresh", paths::data_dir().display());
        return Ok(());
    }
    let put = |c: &str, id: &str, v: Value| -> String {
        let v = model::normalize(c, &v, "USD").unwrap_or_else(|e| panic!("{c}: {e} in {v}"));
        s.put(c, Some(id), &v).unwrap().id
    };
    fx::save_fetched(&s, "USD", &json!({"BDT": 120.0, "EUR": 0.92, "GBP": 0.79, "INR": 84.1, "AED": 3.67}))?;
    put(ACCOUNTS, "acc-cash", json!({"name":"Cash","kind":"cash","currency":"USD","opening":180,"order":0}));
    put(ACCOUNTS, "acc-bank", json!({"name":"City Bank","kind":"bank","currency":"USD","opening":5200,"order":1}));
    put(ACCOUNTS, "acc-bkash", json!({"name":"bKash","kind":"wallet","currency":"BDT","opening":30000,"order":2}));
    put(ACCOUNTS, "acc-visa", json!({"name":"Visa","kind":"card","currency":"USD","opening":0,"order":3}));
    let cats = [
        ("cat-food", "Food", "food", "#d9822b", 400.0),
        ("cat-gro", "Groceries", "cart", "#5a9e5f", 300.0),
        ("cat-tr", "Transport", "car", "#3b82c4", 150.0),
        ("cat-cafe", "Coffee", "coffee", "#8c6a4f", 0.0),
        ("cat-bills", "Bills", "bolt", "#c4a43b", 250.0),
        ("cat-shop", "Shopping", "bag", "#a05ac4", 200.0),
        ("cat-health", "Health", "heart", "#d1495b", 0.0),
        ("cat-fun", "Fun", "star", "#e0719c", 120.0),
        ("cat-gift", "Gifts", "gift", "#3b9e9e", 0.0),
        ("cat-home", "Rent", "home", "#5b6cd1", 0.0),
    ];
    for (i, (id, n, ic, col, b)) in cats.iter().enumerate() {
        put(CATEGORIES, id, json!({"name": n, "kind":"expense","icon": ic,"color": col,"budget": b,"order": i}));
    }
    put(CATEGORIES, "cat-old", json!({"name":"Gym","kind":"expense","icon":"fitness","color":"#8a8a8e","order": 20, "archived": true}));
    put(CATEGORIES, "cat-sal", json!({"name":"Salary","kind":"income","icon":"briefcase","color":"#2e7a4d","order":0}));
    put(CATEGORIES, "cat-free", json!({"name":"Freelance","kind":"income","icon":"chart","color":"#3b82c4","order":1}));
    put(CATEGORIES, "cat-ginc", json!({"name":"Gift","kind":"income","icon":"gift","color":"#e0719c","order":2}));

    let today = Local::now().date_naive();
    let start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap() - Duration::days(200);
    let mut seed: u64 = 42;
    let mut rnd = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((seed >> 33) as f64) / ((1u64 << 31) as f64)
    };
    let mut n = 0;
    let mut tx = |v: Value| {
        n += 1;
        put(TRANSACTIONS, &format!("tx-{n:05}"), v);
    };
    let foods = ["Lunch", "Dinner with friends", "Biryani", "Pizza night", "Kacchi Bhai", "Street food", "Sushi"];
    let shops = ["Daraz order", "New shoes", "T-shirt", "Phone case", "Headphones"];
    let mut d = start;
    while d <= today {
        let ds = d.format("%Y-%m-%d").to_string();
        if d.day() == 1 {
            tx(json!({"kind":"income","amount":2300,"date":ds,"time":"09:00","account":"acc-bank","category":"cat-sal","note":"Salary"}));
            tx(json!({"kind":"transfer","amount":200,"date":ds,"time":"10:00","account":"acc-bank","to_account":"acc-bkash","to_amount":24000,"note":"Top up bKash"}));
        }
        if d.day() == 2 || d.day() == 16 {
            tx(json!({"kind":"transfer","amount":220,"date":ds,"time":"11:00","account":"acc-bank","to_account":"acc-cash","note":"ATM"}));
        }
        if d.day() == 25 {
            tx(json!({"kind":"transfer","amount":420,"date":ds,"time":"12:00","account":"acc-bank","to_account":"acc-visa","note":"Card bill"}));
        }
        if d.day() == 15 && rnd() < 0.7 {
            tx(json!({"kind":"income","amount": (300.0 + rnd() * 500.0).round(),"date":ds,"account":"acc-bank","category":"cat-free","note":"Website project"}));
        }
        if d.day() == 5 {
            tx(json!({"kind":"expense","amount":(3500.0 + rnd() * 1200.0).round(),"date":ds,"account":"acc-bkash","currency":"BDT","category":"cat-bills","note":"Electricity","tags":["home"]}));
        }
        if rnd() < 0.75 {
            tx(json!({"kind":"expense","amount": (6.0 + rnd() * 18.0 * 2.0).round() / 2.0,"date":ds,"time": format!("{:02}:{:02}", if d == today { 0 } else { 12 + (rnd()*9.0) as u32 }, (rnd()*59.0) as u32),"account": if rnd() < 0.6 {"acc-cash"} else {"acc-visa"},"category":"cat-food","note": foods[(rnd() * foods.len() as f64) as usize],"tags": if rnd() < 0.2 {json!(["work"])} else {json!([])}}));
        }
        if rnd() < 0.45 {
            tx(json!({"kind":"expense","amount":(4.0 + rnd() * 2.0 * 10.0).round() / 10.0,"date":ds,"time":"08:40","account":"acc-cash","category":"cat-cafe","note":"Flat white"}));
        }
        if rnd() < 0.5 {
            tx(json!({"kind":"expense","amount": (120.0 + rnd() * 500.0).round(),"currency":"BDT","date":ds,"account":"acc-bkash","category":"cat-tr","note": if rnd() < 0.5 {"Pathao ride"} else {"Uber"}}));
        }
        if d.weekday().number_from_monday() == 6 {
            tx(json!({"kind":"expense","amount": (1500.0 + rnd() * 2500.0).round(),"currency":"BDT","date":ds,"account":"acc-bkash","category":"cat-gro","note":"Shwapno"}));
        }
        if rnd() < 0.08 {
            tx(json!({"kind":"expense","amount": (15.0 + rnd() * 80.0).round(),"date":ds,"account":"acc-visa","category":"cat-shop","note": shops[(rnd() * shops.len() as f64) as usize]}));
        }
        if rnd() < 0.06 {
            tx(json!({"kind":"expense","amount": (10.0 + rnd() * 40.0).round(),"date":ds,"account":"acc-visa","category":"cat-fun","note":"Cinema","tags":["friends"]}));
        }
        if rnd() < 0.03 {
            tx(json!({"kind":"expense","amount": (8.0 + rnd() * 30.0).round(),"date":ds,"account":"acc-cash","category":"cat-health","note":"Pharmacy"}));
        }
        if d.day() == 20 && d.month() % 2 == 0 {
            tx(json!({"kind":"expense","amount":60,"date":ds,"account":"acc-visa","splits":[{"category":"cat-shop","amount":35},{"category":"cat-gift","amount":25}],"note":"Mall: shirt + gift for Mum","tags":["family"]}));
        }
        if d < today - Duration::days(120) && d.day() == 10 {
            tx(json!({"kind":"expense","amount":25,"date":ds,"account":"acc-cash","category":"cat-old","note":"Gym membership"}));
        }
        d += Duration::days(1);
    }
    let day = |off: i64| (today + Duration::days(off)).format("%Y-%m-%d").to_string();
    // Recurring + subscriptions (generated up to today).
    let r0 = (today - Duration::days(170)).format("%Y-%m-%d").to_string();
    put(RECURRING, "rec-netflix", json!({"name":"Netflix","freq":"monthly","start":r0,"subscription":true,"template":{"kind":"expense","amount":9.99,"account":"acc-visa","category":"cat-fun"}}));
    put(RECURRING, "rec-spotify", json!({"name":"Spotify","freq":"monthly","start":day(-150),"subscription":true,"template":{"kind":"expense","amount":4.99,"account":"acc-visa","category":"cat-fun"}}));
    put(RECURRING, "rec-net", json!({"name":"Internet","freq":"monthly","start":day(-160),"subscription":true,"template":{"kind":"expense","amount":2400,"currency":"BDT","account":"acc-bkash","category":"cat-bills"}}));
    put(RECURRING, "rec-rent", json!({"name":"Rent","freq":"monthly","start":day(-180),"subscription":false,"template":{"kind":"expense","amount":450,"account":"acc-bank","category":"cat-home"}}));
    put(RECURRING, "rec-icloud", json!({"name":"Cloud storage","freq":"yearly","start":day(-300),"subscription":true,"template":{"kind":"expense","amount":29.99,"account":"acc-visa","category":"cat-bills"}}));
    recur::generate(&s, "USD", today)?;
    // Loans.
    put(LOANS, "loan-rafi", json!({"direction":"lent","person":"Rafi","title":"Laptop repair money","amount":600,"given":day(-60),"due":day(-6),"account":"acc-bank",
        "repayments":[{"date":day(-40),"amount":150,"note":"bKash"},{"date":day(-25),"amount":150},{"date":day(-13),"amount":150,"note":"cash"}],"remind_days":1}));
    put(LOANS, "loan-sam", json!({"direction":"lent","person":"Sam","title":"Concert tickets","amount":120,"given":day(-20),"due":day(4),"repayments":[{"date":day(-5),"amount":40}],"remind_days":1}));
    put(LOANS, "loan-nadia", json!({"direction":"lent","person":"Nadia","title":"Rent share","amount":680,"given":day(-10),"due":day(46),"repayments":[]}));
    put(LOANS, "loan-arif", json!({"direction":"lent","person":"Arif","title":"Bike part","amount":45,"given":day(-90),"due":day(-60),"repayments":[{"date":day(-70),"amount":45}]}));
    put(LOANS, "loan-uncle", json!({"direction":"borrowed","person":"Uncle Karim","title":"Course fee","amount":30000,"currency":"BDT","given":day(-45),"due":day(75),"account":"acc-bkash","repayments":[{"date":day(-15),"amount":10000}],"remind_days":3}));
    // Goals.
    put(GOALS, "goal-phone", json!({"name":"New phone","target":800,"deadline":day(120),"contributions":[{"date":day(-60),"amount":150},{"date":day(-30),"amount":200},{"date":day(-3),"amount":120}]}));
    put(GOALS, "goal-emergency", json!({"name":"Emergency fund","target":3000,"contributions":[{"date":day(-150),"amount":500},{"date":day(-90),"amount":700},{"date":day(-20),"amount":400}]}));
    // SMS waiting for review.
    let now = selfkit_core::time::now_ms();
    for (i, (from, body)) in [
        ("bKash", "Payment of Tk 850.00 to Foodpanda is successful. Balance Tk 12,410.50. TrxID 9KX2ABC1"),
        ("CityBank", "Your A/C XXXX4821 has been debited by BDT 3,200.00 on 14-Nov at AGORA GULSHAN. Available balance BDT 98,120.00"),
    ]
    .iter()
    .enumerate()
    {
        let p = hisab_lib::sms::parse(from, body).unwrap();
        s.put(SMS, None, &json!({"key": format!("seed{i}"), "sender": from, "body": body, "at": now - (i as i64 + 1) * 3_600_000, "status":"pending", "parsed": p}))?;
    }
    s.kv_set("hisab.prefs", &json!({"theme":"system","rank":"manual","notify":true}).to_string())?;
    println!("seeded {} transactions in {}", s.count(TRANSACTIONS)?, paths::data_dir().display());
    Ok(())
}
