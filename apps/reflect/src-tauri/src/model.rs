//! Reflect's numbers and plain sentences: one `Day` row per logical day built
//! from the four apps, then Today, streaks, connections and reviews on top.
//! All aggregation is here (Rust); the UI only lays it out.
//! Docs: docs/apps/reflect/README.md and stats.md.

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::sources::{self, AgendaData, ChronoCache, HisabData, Night};

pub type Ms = i64;
pub const MIN: Ms = 60_000;
pub const DAY: Ms = 86_400_000;

pub fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()
}
pub fn idx_date(i: i32) -> NaiveDate {
    epoch() + Duration::days(i as i64)
}
pub fn date_idx(d: NaiveDate) -> i32 {
    (d - epoch()).num_days() as i32
}
pub fn date_str(i: i32) -> String {
    idx_date(i).format("%Y-%m-%d").to_string()
}

// ------------------------------------------------------------- formatting --

/// "3 h 12", "45 min".
pub fn dur(min: f64) -> String {
    let m = min.max(0.0).round() as i64;
    if m >= 60 { format!("{}\u{a0}h\u{a0}{:02}", m / 60, m % 60) } else { format!("{m}\u{a0}min") }
}
/// Shorter, for sentences: "about 7 h", "50 min".
pub fn dur_round(min: f64) -> String {
    dur(min)
}
/// Whole units for budgets: "$47", "$4.50" under 10.
pub fn money_round(sym: &str, v: f64) -> String {
    if v >= 10.0 { money(sym, v.round()) } else { money(sym, v) }
}
/// "$34.50", "$268", "$1,184", "৳2,400".
pub fn money(sym: &str, v: f64) -> String {
    let neg = v < 0.0;
    let v = v.abs();
    let body = if v < 100.0 && (v - v.round()).abs() > 0.004 {
        format!("{v:.2}")
    } else {
        let n = v.round() as i64;
        let s = n.to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        out
    };
    format!("{}{sym}{body}", if neg { "-" } else { "" })
}
pub fn clock(min: i32) -> String {
    let m = min.rem_euclid(1440);
    format!("{:02}:{:02}", m / 60, m % 60)
}
const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
fn wd(i: i32) -> &'static str {
    WEEKDAYS[idx_date(i).weekday().num_days_from_monday() as usize]
}
fn day_month(i: i32) -> String {
    let d = idx_date(i);
    format!("{} {}", d.day(), MONTHS[d.month0() as usize])
}

// ------------------------------------------------------------------- days --

#[derive(Debug, Clone, Default, Serialize)]
pub struct Day {
    pub date: String,
    #[serde(skip)]
    pub idx: i32,
    /// Minutes on any screen (None: no Chrono, or before its first data).
    pub screen: Option<f64>,
    pub phone: Option<f64>,
    /// Phone minutes between 00:00 and 05:00 that morning.
    pub late: Option<f64>,
    pub spent: Option<f64>,
    /// Spent in categories that have a budget: what `budget` is compared with (C61).
    pub budget_spent: Option<f64>,
    pub budget: Option<f64>,
    pub tasks: Option<f64>,
    /// Last night's sleep (the night that ended this morning), minutes.
    pub sleep: Option<f64>,
    /// Local clock minute of bedtime.
    pub bed: Option<i32>,
    pub quality: Option<u8>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Has {
    pub chrono: bool,
    pub phone: bool,
    pub hisab: bool,
    pub agenda: bool,
    pub dusk: bool,
}

pub struct Ctx<'a> {
    pub days: Vec<Day>,
    pub today: i32,
    pub now: Ms,
    pub tz: i32,
    pub day_start: u16,
    pub sym: String,
    pub has: Has,
    pub chrono: Option<&'a ChronoCache>,
    pub hisab: Option<&'a HisabData>,
    pub agenda: Option<&'a AgendaData>,
    pub nights: &'a [Night],
}

fn days_in_month(d: NaiveDate) -> u32 {
    let (y, m) = (d.year(), d.month());
    let next = if m == 12 { NaiveDate::from_ymd_opt(y + 1, 1, 1) } else { NaiveDate::from_ymd_opt(y, m + 1, 1) }.unwrap();
    (next - NaiveDate::from_ymd_opt(y, m, 1).unwrap()).num_days() as u32
}

/// Build one row per logical day, from the oldest data (at most `max_back`
/// days ago) to today.
pub fn build_days(today: i32, max_back: i32, c: Option<&ChronoCache>, h: Option<&HisabData>, a: Option<&AgendaData>, nights: &[Night]) -> Vec<Day> {
    let night_map: HashMap<i32, &Night> = nights
        .iter()
        .filter_map(|n| Some((date_idx(NaiveDate::parse_from_str(&n.night, "%Y-%m-%d").ok()?), n)))
        .collect();
    let firsts = [c.and_then(|c| c.first), h.and_then(|h| h.first), a.and_then(|a| a.first), night_map.keys().min().copied()];
    let first = firsts.iter().flatten().min().copied().unwrap_or(today).max(today - max_back).min(today);
    (first..=today)
        .map(|i| {
            let mut d = Day { date: date_str(i), idx: i, ..Default::default() };
            if let Some(c) = c
                && c.first.is_some_and(|f| i >= f)
            {
                let cd = c.days.get(&i);
                d.screen = Some(cd.map_or(0.0, |x| x.screen_ms as f64 / MIN as f64));
                if c.has_phone {
                    d.phone = Some(cd.map_or(0.0, |x| x.phone_ms as f64 / MIN as f64));
                    d.late = Some(c.late.get(&i).copied().unwrap_or(0) as f64 / MIN as f64);
                }
            }
            if let Some(h) = h
                && h.first.is_some_and(|f| i >= f)
            {
                d.spent = Some(h.per_day.get(&i).copied().unwrap_or(0.0));
                d.budget_spent = Some(h.budgeted_per_day.get(&i).copied().unwrap_or(0.0));
                if h.monthly_budget > 0.0 {
                    d.budget = Some(h.monthly_budget / days_in_month(idx_date(i)) as f64);
                }
            }
            if let Some(a) = a
                && a.first.is_some_and(|f| i >= f)
            {
                d.tasks = Some(a.per_day.get(&i).copied().unwrap_or(0.0));
            }
            if let Some(n) = night_map.get(&i) {
                match n.source.as_str() {
                    "none" | "" => {}
                    "allnighter" => d.sleep = Some(0.0),
                    _ => {
                        d.sleep = Some(n.asleep_min as f64);
                        d.bed = n.bed.map(|b| ((b + n.tz_offset as i64 * MIN).rem_euclid(DAY) / MIN) as i32);
                    }
                }
                d.quality = n.quality;
            }
            d
        })
        .collect()
}

impl Ctx<'_> {
    pub fn day(&self, i: i32) -> Option<&Day> {
        let f = self.days.first()?.idx;
        if i < f {
            return None;
        }
        self.days.get((i - f) as usize)
    }
    fn range(&self, a: i32, b: i32) -> Vec<&Day> {
        (a..=b).filter_map(|i| self.day(i)).collect()
    }
}

fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

// ------------------------------------------------------------------ today --

#[derive(Debug, Clone, Default, Serialize)]
pub struct Tile {
    pub value: String,
    pub unit: String,
    /// Shown bold before `sub`.
    pub strong: String,
    pub sub: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Ribbon {
    pub from: Ms,
    pub to: Ms,
    pub now: Ms,
    pub first_hour: u16,
    pub sleep: Vec<(Ms, Ms)>,
    pub screen: Vec<(Ms, Ms)>,
    pub money: Vec<Ms>,
    pub tasks: Vec<Ms>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AppRow {
    pub name: String,
    pub device: String,
    pub phone: bool,
    pub min: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Today {
    pub date_label: String,
    pub headline: String,
    pub subline: String,
    pub screen: Option<Tile>,
    pub spent: Option<Tile>,
    pub tasks: Option<Tile>,
    pub sleep: Option<Tile>,
    pub ribbon: Ribbon,
    pub top_apps: Vec<AppRow>,
    pub has: Has,
}

const QUALITY: [&str; 5] = ["awful", "poor", "OK", "good", "great"];

/// Average screen minutes on the same weekday over the last 8 weeks.
fn usual_screen(cx: &Ctx, i: i32) -> Option<f64> {
    mean(&(1..=8).filter_map(|w| cx.day(i - 7 * w)?.screen).filter(|m| *m > 0.0).collect::<Vec<_>>())
}

pub fn today(cx: &Ctx, today_sessions: &[sources::Sess]) -> Today {
    let i = cx.today;
    let d = cx.day(i).cloned().unwrap_or_default();
    let mut t = Today { date_label: format!("{}, {}", wd(i), day_month(i)), has: cx.has, ..Default::default() };
    let local_hour = ((cx.now + cx.tz as i64 * MIN).rem_euclid(DAY) / (60 * MIN)) as i32;
    let evening = local_hour >= 21 || local_hour < cx.day_start as i32 / 60;
    let mut clauses: Vec<String> = Vec::new();
    let mut tone_calm = true;
    let mut title: Option<&str> = None;

    // Screen time
    if let Some(m) = d.screen.filter(|_| cx.has.chrono) {
        let usual = usual_screen(cx, i);
        let (strong, sub) = match usual {
            Some(u) if m > u + 10.0 => (format!("{} more", dur(m - u)), format!("than a usual {}", wd(i))),
            Some(u) if evening && m < u - 10.0 => (format!("{} less", dur(u - m)), format!("than a usual {}", wd(i))),
            Some(u) if (m - u).abs() <= 10.0 && evening => (String::new(), format!("About a usual {}", wd(i))),
            Some(u) => (String::new(), format!("A usual {} is {}", wd(i), dur(u))),
            None => (String::new(), "So far today".into()),
        };
        if let Some(u) = usual {
            if m > u * 1.25 && m - u > 30.0 {
                tone_calm = false;
                title = Some("A busy day on screens.");
                clauses.push("more screen time than usual".into());
            } else if m < u * 0.85 && evening {
                clauses.push("less screen time than usual".into());
            }
        }
        t.screen = Some(Tile { value: dur(m), unit: String::new(), strong, sub });
    }
    // Money
    if let Some(s) = d.spent.filter(|_| cx.has.hisab) {
        let bs = d.budget_spent.unwrap_or(s);
        let sub = match d.budget {
            Some(b) if bs <= b => {
                clauses.push("spending within budget".into());
                format!("Within today's budget of {}", money_round(&cx.sym, b))
            }
            Some(b) => {
                tone_calm = false;
                title = title.or(Some("A spendier day than planned."));
                clauses.push("spending over today's budget".into());
                format!("{} over today's budget", money(&cx.sym, bs - b))
            }
            None if s == 0.0 => "Nothing spent yet".into(),
            None => cx
                .hisab
                .and_then(|h| h.spends.iter().filter(|x| x.day == i).max_by(|a, b| a.amount.total_cmp(&b.amount)))
                .map(|x| format!("Biggest: {} {}", x.category, money(&cx.sym, x.amount)))
                .unwrap_or_default(),
        };
        t.spent = Some(Tile { value: money(&cx.sym, s), unit: String::new(), strong: String::new(), sub });
    }
    // Tasks
    if let Some(n) = d.tasks.filter(|_| cx.has.agenda) {
        let left = cx.agenda.and_then(|a| a.open_by_date.get(&date_str(i)).copied()).unwrap_or(0);
        let sub = match left {
            0 => "Nothing else due today".to_string(),
            1 => "1 left for today".into(),
            k => format!("{k} left for today"),
        };
        if n >= 1.0 {
            clauses.push(format!("{} {} done", n as i64, if n == 1.0 { "task" } else { "tasks" }));
        }
        t.tasks = Some(Tile { value: format!("{}", n as i64), unit: "done".into(), strong: String::new(), sub });
    }
    // Sleep
    let night = cx.nights.iter().rev().find(|n| n.night == date_str(i));
    if cx.has.dusk {
        t.sleep = Some(match (night, d.sleep) {
            (Some(n), Some(m)) if n.source == "allnighter" => {
                let _ = m;
                Tile { value: "No sleep".into(), unit: String::new(), strong: String::new(), sub: "Dusk found no sleep last night".into() }
            }
            (Some(n), Some(m)) => {
                let times = match (n.bed, n.wake) {
                    (Some(b), Some(w)) => format!("{} to {}", clock(((b + n.tz_offset as i64 * MIN).rem_euclid(DAY) / MIN) as i32), clock(((w + n.tz_offset as i64 * MIN).rem_euclid(DAY) / MIN) as i32)),
                    _ => String::new(),
                };
                if m < 360.0 {
                    tone_calm = false;
                    title = title.or(Some("A day after a short night."));
                    clauses.push("a short night behind you".into());
                } else if n.quality.is_some_and(|q| q >= 4) || m >= 420.0 {
                    clauses.push("a good night's sleep behind you".into());
                }
                match n.quality {
                    Some(q) => Tile { value: dur(m), unit: String::new(), strong: format!("Felt {}", QUALITY[(q - 1) as usize]), sub: format!(" · {times}") },
                    None => Tile { value: dur(m), unit: String::new(), strong: String::new(), sub: if n.source == "logged" { times } else { format!("Estimated · {times}") } },
                }
            }
            _ if local_hour < 12 => Tile { value: "—".into(), unit: String::new(), strong: String::new(), sub: "Dusk adds last night once you're up".into() },
            _ => Tile { value: "—".into(), unit: String::new(), strong: String::new(), sub: "No sleep data for last night".into() },
        });
    }
    // Headline
    let any = t.screen.is_some() || t.spent.is_some() || t.tasks.is_some() || t.sleep.is_some();
    let quiet = d.screen.unwrap_or(0.0) < 20.0 && d.tasks.unwrap_or(0.0) == 0.0 && local_hour < 12;
    t.headline = if !any {
        "Nothing to show yet.".into()
    } else if quiet && title.is_none() {
        clauses.retain(|c| c.contains("night"));
        "The day is just starting.".into()
    } else if let Some(x) = title {
        x.into()
    } else if tone_calm && !clauses.is_empty() {
        if evening { "A calm day.".into() } else { "A calm day so far.".into() }
    } else {
        "Here's your day so far.".into()
    };
    t.subline = sentence(&clauses);

    // Ribbon over the logical day.
    let from = sources::day_begin(i, cx.tz, cx.day_start);
    let to = from + DAY;
    t.ribbon = Ribbon { from, to, now: cx.now.min(to), first_hour: cx.day_start / 60, ..Default::default() };
    if cx.has.chrono {
        let end = to.min(cx.now);
        t.ribbon.screen = sources::merge(today_sessions.iter().map(|s| (s.start.max(from), s.end.min(end))).filter(|(a, b)| b > a).collect());
        let mut apps: HashMap<(String, String, bool), Ms> = HashMap::new();
        for s in today_sessions.iter().filter(|s| !s.app.is_empty()) {
            let (a, b) = (s.start.max(from), s.end.min(to.min(cx.now)));
            if b > a {
                *apps.entry((s.app.clone(), s.device.clone(), s.phone)).or_default() += b - a;
            }
        }
        let mut v: Vec<_> = apps.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        t.top_apps = v
            .into_iter()
            .take(5)
            .filter(|x| x.1 >= MIN)
            .map(|((name, device, phone), ms)| AppRow { name, device, phone, min: (ms / MIN) as f64 })
            .collect();
    }
    if let Some(n) = night
        && let (Some(b), Some(w)) = (n.bed, n.wake)
    {
        let (a, z) = (b.max(from), w.min(to));
        if z > a {
            t.ribbon.sleep.push((a, z));
        }
    }
    if let Some(h) = cx.hisab {
        let cal0 = sources::day_begin(date_idx(idx_date(i)), cx.tz, 0);
        for s in h.spends.iter().filter(|s| s.day == i) {
            if let Some(m) = s.minute {
                let off = if m < cx.day_start { DAY } else { 0 };
                t.ribbon.money.push(cal0 + off + m as i64 * MIN);
            }
        }
    }
    if let Some(a) = cx.agenda {
        t.ribbon.tasks = a.done.iter().filter(|x| x.0 == i && x.1 <= cx.now).map(|x| x.1).collect();
    }
    t
}

/// "A, b and c." with the first letter upper-cased.
fn sentence(parts: &[String]) -> String {
    if parts.is_empty() {
        return String::new();
    }
    let mut s = match parts.len() {
        1 => parts[0].clone(),
        n => format!("{}, and {}", parts[..n - 1].join(", "), parts[n - 1]),
    };
    if parts.len() == 2 {
        s = format!("{} and {}", parts[0], parts[1]);
    }
    let mut c = s.chars();
    match c.next() {
        Some(f) => format!("{}{}.", f.to_uppercase(), c.as_str()),
        None => s,
    }
}

// ---------------------------------------------------------------- streaks --

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreakDef {
    pub id: String,
    /// phone_under | screen_under | budget | asleep_by | tasks_min
    pub kind: String,
    /// Minutes (phone/screen), minute of the day (asleep_by), count (tasks). Unused for budget.
    pub value: f64,
    pub on: bool,
}

pub fn default_streaks() -> Vec<StreakDef> {
    let s = |id: &str, kind: &str, value: f64, on: bool| StreakDef { id: id.into(), kind: kind.into(), value, on };
    vec![s("phone", "phone_under", 180.0, true), s("budget", "budget", 0.0, true), s("sleep", "asleep_by", 0.0, true), s("tasks", "tasks_min", 3.0, true), s("screen", "screen_under", 300.0, false)]
}

pub fn streak_name(d: &StreakDef) -> String {
    match d.kind.as_str() {
        "phone_under" => format!("Phone under {}", dur_short(d.value)),
        "screen_under" => format!("All screens under {}", dur_short(d.value)),
        "budget" => "Spending within budget".into(),
        "asleep_by" => {
            if d.value as i32 % 1440 == 0 { "Asleep by midnight".into() } else { format!("Asleep by {}", clock(d.value as i32)) }
        }
        "tasks_min" => format!("At least {} {} done", d.value as i64, if d.value == 1.0 { "task" } else { "tasks" }),
        _ => d.kind.clone(),
    }
}
/// "3 h", "2 h 30", "45 min".
pub fn dur_short(min: f64) -> String {
    let m = min.round() as i64;
    match (m / 60, m % 60) {
        (0, r) => format!("{r} min"),
        (h, 0) => format!("{h} h"),
        (h, r) => format!("{h} h {r:02}"),
    }
}
pub fn streak_app(kind: &str) -> &'static str {
    match kind {
        "phone_under" | "screen_under" => "chrono",
        "budget" => "hisab",
        "asleep_by" => "dusk",
        _ => "agenda",
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kept {
    Yes,
    No,
    /// Today, not decided yet.
    Pending,
    NoData,
}

fn kept(def: &StreakDef, d: &Day, is_today: bool) -> Kept {
    use Kept::*;
    let r = match def.kind.as_str() {
        "phone_under" => d.phone.map(|m| m < def.value),
        "screen_under" => d.screen.map(|m| m < def.value),
        "budget" => match (d.budget_spent.or(d.spent), d.budget) {
            (Some(s), Some(b)) => Some(s <= b),
            _ => None,
        },
        "asleep_by" => match (d.sleep, d.bed) {
            (Some(_), Some(b)) => Some((b - 18 * 60).rem_euclid(1440) <= (def.value as i32 - 18 * 60).rem_euclid(1440)),
            (Some(_), None) => Some(false),
            _ => None,
        },
        "tasks_min" => d.tasks.map(|n| n >= def.value),
        _ => None,
    };
    match (r, is_today) {
        (None, true) if def.kind != "asleep_by" => Pending,
        (None, _) => NoData,
        (Some(true), true) if def.kind != "tasks_min" && def.kind != "asleep_by" => Pending,
        (Some(false), true) if def.kind == "tasks_min" => Pending,
        (Some(true), _) => Yes,
        (Some(false), _) => No,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StreakView {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub app: String,
    pub on: bool,
    pub value: f64,
    pub current: u32,
    pub best: u32,
    /// Last 30 days, oldest first: k kept · x missed · t today (open) · n no data.
    pub pattern: Vec<String>,
    pub kept: u32,
    pub of: u32,
    pub note: String,
    pub available: bool,
    pub missing: String,
}

fn app_name(a: &str) -> &'static str {
    match a {
        "chrono" => "Chrono",
        "hisab" => "Hisab",
        "dusk" => "Dusk",
        _ => "Agenda",
    }
}

fn source_has(has: &Has, kind: &str) -> bool {
    match kind {
        "phone_under" => has.phone,
        "screen_under" => has.chrono,
        "budget" => has.hisab,
        "asleep_by" => has.dusk,
        _ => has.agenda,
    }
}

pub fn streak(cx: &Ctx, def: &StreakDef) -> StreakView {
    let app = streak_app(&def.kind);
    let mut v = StreakView {
        id: def.id.clone(),
        kind: def.kind.clone(),
        name: streak_name(def),
        app: app.into(),
        on: def.on,
        value: def.value,
        current: 0,
        best: 0,
        pattern: vec![],
        kept: 0,
        of: 0,
        note: String::new(),
        available: source_has(&cx.has, &def.kind),
        missing: String::new(),
    };
    if !v.available {
        v.missing = if cfg!(mobile) {
            format!("{} data isn't shared with phones yet", app_name(app))
        } else if def.kind == "phone_under" && cx.has.chrono {
            "No phone data in Chrono yet".into()
        } else {
            format!("Needs {}", app_name(app))
        };
        return v;
    }
    if def.kind == "budget" && cx.hisab.is_some_and(|h| h.monthly_budget <= 0.0) {
        v.available = false;
        v.missing = "Set monthly budgets in Hisab to use this".into();
        return v;
    }
    let states: Vec<Kept> = cx.days.iter().map(|d| kept(def, d, d.idx == cx.today)).collect();
    // Best run.
    let mut run = 0;
    for s in &states {
        match s {
            Kept::Yes => {
                run += 1;
                v.best = v.best.max(run);
            }
            Kept::Pending => {}
            _ => run = 0,
        }
    }
    // Current run, back from today.
    for s in states.iter().rev() {
        match s {
            Kept::Yes => v.current += 1,
            Kept::Pending => continue,
            _ => break,
        }
    }
    let n = states.len();
    let last: Vec<Kept> = states[n.saturating_sub(30)..].to_vec();
    let pad = 30 - last.len();
    v.pattern = std::iter::repeat_n("n".to_string(), pad)
        .chain(last.iter().map(|s| match s {
            Kept::Yes => "k",
            Kept::No => "x",
            Kept::Pending => "t",
            Kept::NoData => "n",
        }.to_string()))
        .collect();
    v.kept = last.iter().filter(|s| **s == Kept::Yes).count() as u32;
    v.of = last.iter().filter(|s| matches!(s, Kept::Yes | Kept::No)).count() as u32;
    let unit = if def.kind == "asleep_by" { "nights" } else { "days" };
    v.note = if def.kind == "budget" {
        let b = cx.days.last().and_then(|d| d.budget).unwrap_or(0.0);
        format!("Budget {} a day", money_round(&cx.sym, b))
    } else {
        format!("Kept {} of the last {} {unit}", v.kept, v.of)
    };
    v
}

// ------------------------------------------------------------ connections --

#[derive(Debug, Clone, Default, Serialize)]
pub struct Group {
    pub label: String,
    pub value: f64,
    pub n: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Bucket {
    pub label: String,
    pub value: Option<f64>,
    pub n: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Conn {
    pub id: String,
    pub topic: String,
    pub finding: String,
    pub strength: u8,
    /// [highlighted, other]
    pub groups: Vec<Group>,
    pub buckets: Vec<Bucket>,
    pub r: Option<f64>,
    pub pairs: usize,
    /// days | nights
    pub per: String,
    pub apps: Vec<String>,
    /// min | money | tasks
    pub unit: String,
    pub measure: String,
    pub available: bool,
    pub missing: String,
    pub enough: bool,
}

pub fn pearson(xs: &[f64], ys: &[f64]) -> Option<f64> {
    if xs.len() < 5 || xs.len() != ys.len() {
        return None;
    }
    let (mx, my) = (mean(xs)?, mean(ys)?);
    let cov: f64 = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    let vx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    let vy: f64 = ys.iter().map(|y| (y - my).powi(2)).sum();
    (vx > 0.0 && vy > 0.0).then(|| (cov / (vx * vy).sqrt() * 100.0).round() / 100.0)
}

pub fn strength(r: Option<f64>) -> u8 {
    match r.map(f64::abs) {
        Some(x) if x >= 0.5 => 3,
        Some(x) if x >= 0.3 => 2,
        Some(x) if x >= 0.15 => 1,
        _ => 0,
    }
}

struct Spec {
    id: &'static str,
    topic: &'static str,
    apps: [&'static str; 2],
    per: &'static str,
    unit: &'static str,
    measure: &'static str,
    /// Split x: group A = x ≥ split when a_high, else x < split.
    split: f64,
    a_high: bool,
    labels: [String; 2],
    edges: Vec<(f64, f64, String)>,
}

fn connection(spec: Spec, pairs: &[(f64, f64)], available: bool, missing: String, finding: impl Fn(f64, f64, u8) -> String) -> Conn {
    let mut c = Conn {
        id: spec.id.into(),
        topic: spec.topic.into(),
        per: spec.per.into(),
        apps: spec.apps.iter().map(|s| s.to_string()).collect(),
        unit: spec.unit.into(),
        measure: spec.measure.into(),
        available,
        missing,
        pairs: pairs.len(),
        ..Default::default()
    };
    if !available {
        return c;
    }
    let (xs, ys): (Vec<f64>, Vec<f64>) = pairs.iter().copied().unzip();
    c.r = pearson(&xs, &ys);
    let ga: Vec<f64> = pairs.iter().filter(|p| (p.0 >= spec.split) == spec.a_high).map(|p| p.1).collect();
    let gb: Vec<f64> = pairs.iter().filter(|p| (p.0 >= spec.split) != spec.a_high).map(|p| p.1).collect();
    c.groups = vec![
        Group { label: spec.labels[0].clone(), value: mean(&ga).unwrap_or(0.0), n: ga.len() },
        Group { label: spec.labels[1].clone(), value: mean(&gb).unwrap_or(0.0), n: gb.len() },
    ];
    c.buckets = spec
        .edges
        .iter()
        .map(|(lo, hi, l)| {
            let v: Vec<f64> = pairs.iter().filter(|p| p.0 >= *lo && p.0 < *hi).map(|p| p.1).collect();
            Bucket { label: l.clone(), value: mean(&v), n: v.len() }
        })
        .collect();
    c.enough = ga.len() >= 3 && gb.len() >= 3 && c.r.is_some();
    c.strength = if c.enough { strength(c.r) } else { 0 };
    c.finding = if c.enough {
        finding(c.groups[0].value, c.groups[1].value, c.strength)
    } else {
        format!("Not enough {} yet to compare. Keep using both apps.", spec.per)
    };
    c
}

fn more_less(a: f64, b: f64, unit: &str, sym: &str) -> String {
    let d = (a - b).abs();
    match unit {
        "min" => dur_round(d),
        "money" => money_round(sym, d),
        _ => format!("{d:.1}"),
    }
}

/// Connections over the last `lookback` finished days.
pub fn connections(cx: &Ctx, lookback: i32) -> Vec<Conn> {
    let days = cx.range(cx.today - lookback, cx.today - 1);
    let has = cx.has;
    let sym = cx.sym.clone();
    let mut out = Vec::new();

    // 1. Late phone use → sleep (same night).
    {
        let pairs: Vec<(f64, f64)> = days.iter().filter_map(|d| Some((d.late?, d.sleep.filter(|s| *s > 0.0)?))).collect();
        let avail = has.phone && has.dusk;
        let miss = if !has.dusk { "Needs Dusk".into() } else { "Needs phone data in Chrono".into() };
        out.push(connection(
            Spec {
                id: "late_sleep",
                topic: "Late phone use & sleep",
                apps: ["chrono", "dusk"],
                per: "nights",
                unit: "min",
                measure: "Sleep, split by phone use after midnight",
                split: 30.0,
                a_high: true,
                labels: ["30+ min on the phone after midnight".into(), "Other nights".into()],
                edges: vec![(0.0, 1.0, "No phone after midnight".into()), (1.0, 30.0, "Under 30 min".into()), (30.0, 60.0, "30–60 min".into()), (60.0, 1e9, "Over 1 hour".into())],
            },
            &pairs,
            avail,
            miss,
            |a, b, s| {
                if s == 0 || (a - b).abs() < 10.0 {
                    "Late phone use doesn't seem to change how long you sleep.".into()
                } else if a < b {
                    format!("You sleep about {} less after late phone use.", dur_round(b - a))
                } else {
                    format!("You sleep about {} more after late phone use.", dur_round(a - b))
                }
            },
        ));
    }
    // 2. Screen time → tasks done (same day). Split at the median.
    {
        let pairs: Vec<(f64, f64)> = days.iter().filter_map(|d| Some((d.screen.filter(|m| *m > 0.0)?, d.tasks?))).collect();
        let mut xs: Vec<f64> = pairs.iter().map(|p| p.0).collect();
        xs.sort_by(f64::total_cmp);
        let med = xs.get(xs.len() / 2).copied().unwrap_or(240.0);
        let med = ((med / 30.0).round() * 30.0).max(30.0);
        let q = |f: f64| xs.get(((xs.len() as f64 * f) as usize).min(xs.len().saturating_sub(1))).copied().unwrap_or(0.0);
        let (q1, q3) = (((q(0.25) / 30.0).round() * 30.0).max(30.0), ((q(0.75) / 30.0).round() * 30.0).max(60.0));
        out.push(connection(
            Spec {
                id: "screen_tasks",
                topic: "Screen time & tasks",
                apps: ["chrono", "agenda"],
                per: "days",
                unit: "tasks",
                measure: "Tasks done, split by screen time that day",
                split: med,
                a_high: false,
                labels: [format!("Under {} on screens", dur_short(med)), format!("{} or more", dur_short(med))],
                edges: vec![
                    (0.0, q1, format!("Under {}", dur_short(q1))),
                    (q1, med.max(q1 + 1.0), format!("{} to {}", dur_short(q1), dur_short(med))),
                    (med.max(q1 + 1.0), q3.max(med + 1.0), format!("{} to {}", dur_short(med), dur_short(q3))),
                    (q3.max(med + 1.0), 1e9, format!("Over {}", dur_short(q3))),
                ],
            },
            &pairs,
            has.chrono && has.agenda,
            if !has.agenda { "Needs Agenda".into() } else { "Needs Chrono".into() },
            |a, b, s| {
                if s == 0 || (a - b).abs() < 0.3 {
                    "Screen time doesn't seem to change how much you get done.".into()
                } else if a > b {
                    "You finish more tasks on lighter screen days.".into()
                } else {
                    "You finish more tasks on busier screen days.".into()
                }
            },
        ));
    }
    // 3. Late phone use (this morning) → spending that day.
    {
        let pairs: Vec<(f64, f64)> = days.iter().filter_map(|d| Some((d.late?, d.spent?))).collect();
        let s2 = sym.clone();
        out.push(connection(
            Spec {
                id: "late_spend",
                topic: "Late phone use & next-day spending",
                apps: ["chrono", "hisab"],
                per: "days",
                unit: "money",
                measure: "Spent the next day, split by phone use after midnight",
                split: 30.0,
                a_high: true,
                labels: ["After a late night".into(), "Other days".into()],
                edges: vec![(0.0, 1.0, "No phone after midnight".into()), (1.0, 30.0, "Under 30 min".into()), (30.0, 60.0, "30–60 min".into()), (60.0, 1e9, "Over 1 hour".into())],
            },
            &pairs,
            has.phone && has.hisab,
            if !has.hisab { "Needs Hisab".into() } else { "Needs phone data in Chrono".into() },
            move |a, b, s| {
                if s == 0 || (a - b).abs() < 0.08 * a.max(b).max(1.0) {
                    "Late nights don't seem to change your spending.".into()
                } else if a > b {
                    format!("You spend about {} more after a late night.", more_less(a, b, "money", &s2))
                } else {
                    format!("You spend about {} less after a late night.", more_less(a, b, "money", &s2))
                }
            },
        ));
    }
    // 4. Sleep → screen time that day.
    {
        let pairs: Vec<(f64, f64)> = days.iter().filter_map(|d| Some((d.sleep.filter(|s| *s > 0.0)?, d.screen.filter(|m| *m > 0.0)?))).collect();
        out.push(connection(
            Spec {
                id: "sleep_screen",
                topic: "Sleep & next-day screen time",
                apps: ["dusk", "chrono"],
                per: "days",
                unit: "min",
                measure: "Screen time, split by sleep the night before",
                split: 420.0,
                a_high: false,
                labels: ["After less than 7 h".into(), "After 7 h or more".into()],
                edges: vec![(0.0, 360.0, "Under 6 h".into()), (360.0, 420.0, "6 to 7 h".into()), (420.0, 480.0, "7 to 8 h".into()), (480.0, 1e9, "8 h or more".into())],
            },
            &pairs,
            has.dusk && has.chrono,
            if !has.dusk { "Needs Dusk".into() } else { "Needs Chrono".into() },
            |a, b, s| {
                if s == 0 || (a - b).abs() < 15.0 {
                    "Short nights don't seem to change your screen time.".into()
                } else if a > b {
                    format!("After short nights you spend about {} more on screens.", dur_round(a - b))
                } else {
                    format!("After short nights you spend about {} less on screens.", dur_round(b - a))
                }
            },
        ));
    }
    // 5. Sleep → spending that day.
    {
        let pairs: Vec<(f64, f64)> = days.iter().filter_map(|d| Some((d.sleep.filter(|s| *s > 0.0)?, d.spent?))).collect();
        let s2 = sym.clone();
        out.push(connection(
            Spec {
                id: "sleep_spend",
                topic: "Sleep & spending",
                apps: ["dusk", "hisab"],
                per: "days",
                unit: "money",
                measure: "Spent, split by sleep the night before",
                split: 420.0,
                a_high: false,
                labels: ["After less than 7 h".into(), "After 7 h or more".into()],
                edges: vec![(0.0, 360.0, "Under 6 h".into()), (360.0, 420.0, "6 to 7 h".into()), (420.0, 480.0, "7 to 8 h".into()), (480.0, 1e9, "8 h or more".into())],
            },
            &pairs,
            has.dusk && has.hisab,
            if !has.dusk { "Needs Dusk".into() } else { "Needs Hisab".into() },
            move |a, b, s| {
                if s == 0 || (a - b).abs() < 0.08 * a.max(b).max(1.0) {
                    "Short nights don't seem to change your spending.".into()
                } else if a > b {
                    format!("You may spend about {} more after short nights.", more_less(a, b, "money", &s2))
                } else {
                    format!("You spend about {} less after short nights.", more_less(a, b, "money", &s2))
                }
            },
        ));
    }
    if cfg!(mobile) {
        for c in out.iter_mut().filter(|c| !c.available) {
            c.missing = "Not shared with phones yet".into();
        }
    }
    // Strongest first; unavailable last.
    out.sort_by_key(|c| (!c.available, !c.enough, std::cmp::Reverse(c.strength)));
    out
}

// ---------------------------------------------------------------- reviews --

#[derive(Debug, Clone, Default, Serialize)]
pub struct Sect {
    pub value: String,
    pub unit: String,
    pub sub: String,
    /// One value per day of the period (None = no data / future).
    pub series: Vec<Option<f64>>,
    pub goal: Option<f64>,
    pub goal_label: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Worth {
    pub icon: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct KeptRow {
    pub name: String,
    pub pattern: Vec<String>,
    pub kept: u32,
    pub of: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Review {
    pub kind: String,
    pub offset: i32,
    pub label: String,
    pub short_label: String,
    pub range_label: String,
    pub in_progress: bool,
    pub can_next: bool,
    pub can_prev: bool,
    pub headline: String,
    pub sub: String,
    /// Day labels for the bars ("M", "T", … or day numbers).
    pub day_labels: Vec<String>,
    pub dates: Vec<String>,
    /// Monday-first offset of the first day (month calendar).
    pub lead: u32,
    pub screen: Option<Sect>,
    pub spent: Option<Sect>,
    pub tasks: Option<Sect>,
    pub sleep: Option<Sect>,
    pub worth: Vec<Worth>,
    pub kept: Vec<KeptRow>,
    pub top_apps: Vec<AppRow>,
    pub empty: bool,
}

/// [first, last] day index of a week/month `offset` periods from the current one.
pub fn period(kind: &str, offset: i32, today: i32, week_start: u8) -> (i32, i32) {
    let d = idx_date(today);
    if kind == "month" {
        let mut y = d.year();
        let mut m = d.month() as i32 - 1 + offset;
        y += m.div_euclid(12);
        m = m.rem_euclid(12);
        let first = NaiveDate::from_ymd_opt(y, m as u32 + 1, 1).unwrap();
        (date_idx(first), date_idx(first) + days_in_month(first) as i32 - 1)
    } else {
        let back = (d.weekday().num_days_from_monday() as i32 - week_start as i32).rem_euclid(7);
        let a = today - back + 7 * offset;
        (a, a + 6)
    }
}

/// How far back the Review can go (an offset ≤ -1): to the period holding the
/// oldest data, and always at least one period before the current one (QA
/// 2026-11-15: on "November so far" ← was disabled when data began in
/// November, so October couldn't be opened; empty periods say "No data").
pub fn earliest_offset(cx: &Ctx, kind: &str, week_start: u8) -> i32 {
    let first = cx.days.first().map(|d| d.idx).unwrap_or(cx.today);
    let mut o = -1;
    while o > -600 && period(kind, o, cx.today, week_start).0 > first {
        o -= 1;
    }
    o
}

pub fn review(cx: &Ctx, kind: &str, offset: i32, week_start: u8, defs: &[StreakDef]) -> Review {
    let (a, b) = period(kind, offset, cx.today, week_start);
    let len = (b - a + 1) as usize;
    let (pa, pb) = period(kind, offset - 1, cx.today, week_start);
    let word = if kind == "month" { "month" } else { "week" };
    let mut r = Review { kind: kind.into(), offset, in_progress: b >= cx.today, can_next: b < cx.today, ..Default::default() };
    r.can_prev = offset > earliest_offset(cx, kind, week_start);
    let (da, db) = (idx_date(a), idx_date(b));
    if kind == "month" {
        r.label = format!("{} {}", MONTHS[da.month0() as usize], da.year());
        r.short_label = MONTHS[da.month0() as usize].into();
        r.range_label = if r.in_progress { format!("{} so far", r.label) } else { r.label.clone() };
        r.lead = da.weekday().num_days_from_monday();
        r.day_labels = (1..=len).map(|i| i.to_string()).collect();
    } else {
        let m = |d: NaiveDate| MONTHS[d.month0() as usize];
        r.label = if da.month() == db.month() { format!("{} – {} {}", da.day(), db.day(), m(db)) } else { format!("{} {} – {} {}", da.day(), m(da), db.day(), m(db)) };
        r.short_label = if da.month() == db.month() { format!("{} – {} {}", da.day(), db.day(), &m(db)[..3]) } else { format!("{} {} – {} {}", da.day(), &m(da)[..3], db.day(), &m(db)[..3]) };
        r.range_label = format!("{} {} to {} {}", wd(a), day_month(a), wd(b), day_month(b));
        if r.in_progress {
            r.range_label = format!("This week so far · {}", r.range_label);
        }
        r.day_labels = (a..=b).map(|i| wd(i)[..1].to_string()).collect();
    }
    r.dates = (a..=b).map(date_str).collect();
    let end = b.min(cx.today);
    let get = |i: i32, f: &dyn Fn(&Day) -> Option<f64>| if i > cx.today { None } else { cx.day(i).and_then(f) };
    let series = |f: &dyn Fn(&Day) -> Option<f64>| (a..=b).map(|i| get(i, f)).collect::<Vec<_>>();
    let avg_of = |x: i32, y: i32, f: &dyn Fn(&Day) -> Option<f64>| mean(&(x..=y.min(cx.today)).filter_map(|i| get(i, f)).collect::<Vec<_>>());
    let sum_of = |x: i32, y: i32, f: &dyn Fn(&Day) -> Option<f64>| {
        let v: Vec<f64> = (x..=y.min(cx.today)).filter_map(|i| get(i, f)).collect();
        (!v.is_empty()).then(|| v.iter().sum::<f64>())
    };
    let prev_word = if kind == "month" { MONTHS[idx_date(pa).month0() as usize].to_string() } else { "the week before".into() };
    let mut clauses: Vec<String> = Vec::new();
    let mut title = format!("A steady {word}.");

    // Screen
    let fs: &dyn Fn(&Day) -> Option<f64> = &|d: &Day| d.screen;
    if cx.has.chrono
        && let Some(avg) = avg_of(a, end, fs)
    {
        let sub = match avg_of(pa, pb, fs) {
            Some(p) if p > 0.0 => {
                let pct = (avg - p) / p;
                if pct <= -0.08 {
                    title = format!("A lighter {word} on screens.");
                } else if pct >= 0.08 {
                    title = format!("A heavier {word} on screens.");
                }
                if (avg - p).abs() < 5.0 {
                    format!("About the same as {prev_word}")
                } else if kind == "month" {
                    format!("{}% {} than {prev_word}", (pct.abs() * 100.0).round(), if avg < p { "less" } else { "more" })
                } else {
                    format!("{} {} than {prev_word}", dur(( avg - p).abs()), if avg < p { "less" } else { "more" })
                }
            }
            _ => "Your first full period with data".into(),
        };
        let phone_def = defs.iter().find(|d| d.on && d.kind == "phone_under");
        r.screen = Some(Sect {
            value: dur(avg),
            unit: "a day".into(),
            sub,
            series: series(fs).into_iter().map(|v| v.map(|m| m / 60.0)).collect(),
            goal: phone_def.filter(|_| cx.has.phone).map(|d| d.value / 60.0).or(defs.iter().find(|d| d.on && d.kind == "screen_under").map(|d| d.value / 60.0)),
            goal_label: phone_def.filter(|_| cx.has.phone).map(|d| format!("Phone {}", dur_short(d.value))).unwrap_or_else(|| defs.iter().find(|d| d.on && d.kind == "screen_under").map(|d| dur_short(d.value)).unwrap_or_default()),
        });
        if r.screen.as_ref().is_some_and(|s| s.goal_label.starts_with("Phone")) {
            // The goal line is for the phone only; bars show all screens. Hide to avoid mixing.
            if let Some(s) = r.screen.as_mut() {
                s.goal = None;
                s.goal_label.clear();
            }
        }
    }
    // Money
    let fm: &dyn Fn(&Day) -> Option<f64> = &|d: &Day| d.spent;
    if cx.has.hisab
        && let Some(total) = sum_of(a, end, fm)
    {
        let budget: f64 = (a..=end).filter_map(|i| cx.day(i).and_then(|d| d.budget)).sum();
        // Only spending in budgeted categories counts against the budget (C61).
        let btotal: f64 = (a..=end).filter_map(|i| cx.day(i).and_then(|d| d.budget_spent.or(d.spent))).sum();
        let kept_days = (a..=end).filter(|i| cx.day(*i).is_some_and(|d| matches!((d.budget_spent.or(d.spent), d.budget), (Some(s), Some(b)) if s <= b))).count();
        let n_days = (a..=end).filter(|i| cx.day(*i).is_some_and(|d| d.spent.is_some())).count();
        let sub = if budget > 0.0 {
            if btotal <= budget {
                clauses.push(format!("kept your budget on {kept_days} of {n_days} days"));
                format!("{} under your {word}ly budget", money(&cx.sym, budget - btotal))
            } else {
                clauses.push(format!("went {} over budget", money(&cx.sym, btotal - budget)));
                format!("{} over your {word}ly budget", money(&cx.sym, btotal - budget))
            }
        } else {
            match sum_of(pa, pb, fm) {
                Some(p) if p > 0.0 => format!("{} {} than {prev_word}", money(&cx.sym, (total - p).abs()), if total < p { "less" } else { "more" }),
                _ => String::new(),
            }
        };
        let daily_budget = cx.day(end).and_then(|d| d.budget);
        r.spent = Some(Sect {
            value: money(&cx.sym, total),
            unit: String::new(),
            sub,
            series: series(fm),
            goal: if kind == "week" { daily_budget } else { None },
            goal_label: daily_budget.map(|b| money_round(&cx.sym, b)).unwrap_or_default(),
        });
    }
    // Tasks
    let ft: &dyn Fn(&Day) -> Option<f64> = &|d: &Day| d.tasks;
    if cx.has.agenda
        && let Some(total) = sum_of(a, end, ft)
    {
        let sub = match sum_of(pa, pb, ft) {
            Some(p) if (total - p).abs() >= 1.0 => format!("{} {} than {prev_word}", (total - p).abs() as i64, if total < p { "fewer" } else { "more" }),
            Some(_) => format!("The same as {prev_word}"),
            None => String::new(),
        };
        if total > 0.0 {
            clauses.insert(0, format!("finished {} {}", total as i64, if total == 1.0 { "task" } else { "tasks" }));
        }
        r.tasks = Some(Sect { value: format!("{}", total as i64), unit: if total == 1.0 { "task" } else { "tasks" }.into(), sub, series: series(ft), goal: None, goal_label: String::new() });
    }
    // Sleep
    let fz: &dyn Fn(&Day) -> Option<f64> = &|d: &Day| d.sleep.filter(|m| *m > 0.0);
    if cx.has.dusk
        && let Some(avg) = avg_of(a, end, fz)
    {
        let beds: Vec<f64> = (a..=end).filter_map(|i| cx.day(i).and_then(|d| d.bed)).map(|m| ((m - 18 * 60).rem_euclid(1440)) as f64).collect();
        let sub = mean(&beds).map(|m| format!("Usually asleep around {}", clock(m.round() as i32 + 18 * 60))).unwrap_or_default();
        clauses.push(format!("slept {} a night on average", dur(avg)));
        r.sleep = Some(Sect {
            value: dur(avg),
            unit: "a night".into(),
            sub,
            series: series(fz).into_iter().map(|v| v.map(|m| m / 60.0)).collect(),
            goal: None,
            goal_label: String::new(),
        });
    }
    r.empty = r.screen.is_none() && r.spent.is_none() && r.tasks.is_none() && r.sleep.is_none();
    r.headline = if r.empty { format!("No data for this {word}.") } else { title };
    r.sub = if clauses.is_empty() {
        String::new()
    } else {
        let s = sentence(&clauses);
        format!("You {}{}", s[..1].to_lowercase(), &s[1..])
    };

    // Worth knowing
    let names_day = |i: i32| -> String {
        if kind == "week" { wd(i).to_string() } else { day_month(i) }
    };
    if let Some(c) = cx.chrono.filter(|_| cx.has.chrono) {
        if let Some(i) = (a..=end).max_by_key(|i| c.days.get(i).map_or(0, |d| d.screen_ms))
            && let Some(d) = c.days.get(&i)
            && d.screen_ms > 0
        {
            let top = d.apps.first().map(|x| format!(", mostly {}", x.0)).unwrap_or_default();
            r.worth.push(Worth { icon: "phone".into(), text: format!("{} was your heaviest screen day: {}{top}.", names_day(i), dur(d.screen_ms as f64 / MIN as f64)) });
        }
        // Top apps over the period.
        let mut tot: HashMap<(String, String), Ms> = HashMap::new();
        for i in a..=end {
            if let Some(d) = c.days.get(&i) {
                for (ap, dv, ms) in &d.apps {
                    *tot.entry((ap.clone(), dv.clone())).or_default() += ms;
                }
            }
        }
        let mut v: Vec<_> = tot.into_iter().collect();
        v.sort_by(|x, y| y.1.cmp(&x.1));
        let ndays = (end - a + 1).max(1) as f64;
        r.top_apps = v.into_iter().take(5).map(|((n, dv), ms)| AppRow { phone: dv == "Phone", name: n, device: dv, min: (ms as f64 / MIN as f64 / ndays).round() }).collect();
    }
    if let Some(h) = cx.hisab.filter(|_| cx.has.hisab) {
        if kind == "week" {
            if let Some(s) = h.spends.iter().filter(|s| s.day >= a && s.day <= end).max_by(|x, y| x.amount.total_cmp(&y.amount)) {
                let what = if s.note.trim().is_empty() { s.category.clone() } else { s.note.trim().to_string() };
                r.worth.push(Worth { icon: "coin".into(), text: format!("Your biggest spend was {}: {} on {}.", wd(s.day), money(&cx.sym, s.amount), what) });
            }
        } else {
            let mut by: HashMap<&str, f64> = HashMap::new();
            for s in h.spends.iter().filter(|s| s.day >= a && s.day <= end) {
                *by.entry(&s.category).or_default() += s.amount;
            }
            let total: f64 = by.values().sum();
            if let Some((c, v)) = by.into_iter().max_by(|x, y| x.1.total_cmp(&y.1))
                && total > 0.0
            {
                let share = v / total;
                let part = if share > 0.45 { "almost half" } else if share > 0.3 { "about a third" } else if share > 0.2 { "about a quarter" } else { "the biggest share" };
                r.worth.push(Worth { icon: "coin".into(), text: format!("{c} took the most money: {}, {part} of the month.", money(&cx.sym, v)) });
            }
        }
    }
    if cx.has.phone {
        let late = (a..=end).filter(|i| cx.day(*i).and_then(|d| d.late).is_some_and(|m| m >= 15.0)).count();
        r.worth.push(Worth {
            icon: "moon".into(),
            text: match late {
                0 => format!("No late nights on your phone this {word}."),
                1 => "You were on your phone after midnight on 1 night.".into(),
                n => format!("You were on your phone after midnight on {n} nights."),
            },
        });
    }
    // Streaks kept
    for def in defs.iter().filter(|d| d.on) {
        let v = streak(cx, def);
        if !v.available {
            continue;
        }
        let pat: Vec<String> = (a..=b)
            .map(|i| match cx.day(i) {
                Some(d) if i <= cx.today => match kept(def, d, i == cx.today) {
                    Kept::Yes => "k",
                    Kept::No => "x",
                    Kept::Pending => "t",
                    Kept::NoData => "n",
                },
                _ => "n",
            }
            .to_string())
            .collect();
        let kept_n = pat.iter().filter(|p| *p == "k").count() as u32;
        let of = pat.iter().filter(|p| *p == "k" || *p == "x").count() as u32;
        if kind == "month" {
            // Longest run inside the month.
            let (mut best, mut run, mut best_end) = (0, 0, a);
            for (k, p) in pat.iter().enumerate() {
                if p == "k" {
                    run += 1;
                    if run > best {
                        best = run;
                        best_end = a + k as i32;
                    }
                } else {
                    run = 0;
                }
            }
            if best >= 3 && r.worth.len() < 4 && def.kind == defs.iter().find(|d| d.on).map(|d| d.kind.clone()).unwrap_or_default() {
                let s = best_end - best + 1;
                r.worth.insert(0, Worth { icon: "streak".into(), text: format!("Your longest run of “{}” this month was {best} days, {} to {}.", streak_name(def).to_lowercase(), idx_date(s).day(), day_month(best_end)) });
            }
        }
        r.kept.push(KeptRow { name: streak_name(def), pattern: pat, kept: kept_n, of });
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(i: i32) -> Day {
        Day { date: date_str(i), idx: i, ..Default::default() }
    }

    fn cx_with(days: Vec<Day>, today: i32) -> Ctx<'static> {
        Ctx {
            days,
            today,
            now: sources::day_begin(today, 0, 0) + 20 * 60 * MIN,
            tz: 0,
            day_start: 0,
            sym: "$".into(),
            has: Has { chrono: true, phone: true, hisab: true, agenda: true, dusk: true },
            chrono: None,
            hisab: None,
            agenda: None,
            nights: &[],
        }
    }

    #[test]
    fn formats() {
        assert_eq!(dur(192.0), "3\u{a0}h\u{a0}12");
        assert_eq!(dur(45.0), "45\u{a0}min");
        assert_eq!(money("$", 34.5), "$34.50");
        assert_eq!(money("$", 268.0), "$268");
        assert_eq!(money("$", 1184.0), "$1,184");
        assert_eq!(dur_short(180.0), "3 h");
        assert_eq!(clock(-10), "23:50");
        assert_eq!(sentence(&["a".into(), "b".into(), "c".into()]), "A, b, and c.");
        assert_eq!(sentence(&["less screen".into(), "spending within budget".into()]), "Less screen and spending within budget.");
    }

    #[test]
    fn streaks_count_runs_and_today() {
        let today = 20_000;
        let mut days: Vec<Day> = (today - 9..=today).map(d).collect();
        // phone minutes: kept except day -5; today 100 (under, still open).
        for (k, x) in days.iter_mut().enumerate() {
            x.phone = Some(if k == 4 { 250.0 } else { 100.0 });
            x.tasks = Some(if k == 9 { 1.0 } else { 3.0 });
        }
        let cx = cx_with(days, today);
        let phone = StreakDef { id: "p".into(), kind: "phone_under".into(), value: 180.0, on: true };
        let v = streak(&cx, &phone);
        assert_eq!(v.current, 4, "yesterday back to the missed day; today is open");
        assert_eq!(v.best, 4);
        assert_eq!(v.pattern.last().unwrap(), "t");
        assert_eq!(v.pattern.len(), 30);
        assert_eq!((v.kept, v.of), (8, 9));
        let tasks = StreakDef { id: "t".into(), kind: "tasks_min".into(), value: 3.0, on: true };
        let v = streak(&cx, &tasks);
        assert_eq!(v.current, 9, "today not reached yet → pending, run continues");
        let tasks2 = StreakDef { value: 1.0, ..tasks };
        assert_eq!(streak(&cx, &tasks2).current, 10, "today reached → counts");
    }

    #[test]
    fn asleep_by_wraps_midnight() {
        let def = StreakDef { id: "s".into(), kind: "asleep_by".into(), value: 0.0, on: true };
        let mut x = d(5);
        x.sleep = Some(420.0);
        x.bed = Some(23 * 60 + 40);
        assert_eq!(kept(&def, &x, false), Kept::Yes);
        x.bed = Some(30);
        assert_eq!(kept(&def, &x, false), Kept::No);
        x.bed = Some(0);
        assert_eq!(kept(&def, &x, false), Kept::Yes);
    }

    #[test]
    fn connections_find_a_link() {
        let today = 20_000;
        let days: Vec<Day> = (today - 90..=today)
            .map(|i| {
                let mut x = d(i);
                let late = if i % 3 == 0 { 60.0 } else { 0.0 };
                x.late = Some(late);
                x.phone = Some(200.0);
                x.screen = Some(200.0 + (i % 5) as f64 * 40.0);
                x.sleep = Some(if late > 0.0 { 380.0 } else { 440.0 } + (i % 4) as f64);
                x.tasks = Some(if x.screen.unwrap() < 280.0 { 4.0 } else { 2.0 });
                x.spent = Some(30.0 + (i % 7) as f64);
                x
            })
            .collect();
        let cx = cx_with(days, today);
        let c = connections(&cx, 90);
        let ls = c.iter().find(|c| c.id == "late_sleep").unwrap();
        assert!(ls.enough && ls.strength == 3, "{ls:?}");
        assert!(ls.finding.contains("less after late phone use"), "{}", ls.finding);
        let st = c.iter().find(|c| c.id == "screen_tasks").unwrap();
        assert!(st.finding.contains("lighter screen days"), "{}", st.finding);
        let sp = c.iter().find(|c| c.id == "late_spend").unwrap();
        assert!(sp.strength <= 1, "spending is noise: {sp:?}");
        assert_eq!(c[0].strength, 3, "strongest first");
    }

    #[test]
    fn periods() {
        // 2026-11-15 is a Sunday.
        let t = date_idx(NaiveDate::from_ymd_opt(2026, 11, 15).unwrap());
        let (a, b) = period("week", -1, t, 0);
        assert_eq!((date_str(a), date_str(b)), ("2026-11-02".into(), "2026-11-08".into()));
        let (a, _) = period("week", 0, t, 6);
        assert_eq!(date_str(a), "2026-11-15", "Sunday-first week starts today");
        let (a, b) = period("month", -1, t, 0);
        assert_eq!((date_str(a), date_str(b)), ("2026-10-01".into(), "2026-10-31".into()));
        let (a, _) = period("month", -11, t, 0);
        assert_eq!(date_str(a), "2025-12-01");
    }

    #[test]
    fn weekly_review_sentences() {
        let t = date_idx(NaiveDate::from_ymd_opt(2026, 11, 15).unwrap());
        let days: Vec<Day> = (t - 30..=t)
            .map(|i| {
                let mut x = d(i);
                x.screen = Some(if i < t - 13 { 300.0 } else { 220.0 });
                x.tasks = Some(3.0);
                x.spent = Some(30.0);
                x.budget = Some(40.0);
                x.sleep = Some(420.0);
                x.bed = Some(23 * 60 + 50);
                x
            })
            .collect();
        let cx = cx_with(days, t);
        let r = review(&cx, "week", -1, 0, &default_streaks());
        assert_eq!(r.label, "2 – 8 November");
        assert_eq!(r.headline, "A lighter week on screens.");
        assert_eq!(r.tasks.as_ref().unwrap().value, "21");
        assert_eq!(r.spent.as_ref().unwrap().sub, "$70 under your weekly budget");
        assert!(r.sub.starts_with("You finished 21 tasks"), "{}", r.sub);
        assert_eq!(r.sleep.as_ref().unwrap().sub, "Usually asleep around 23:50");
        assert!(r.can_next && !r.in_progress);
    }

    /// QA 2026-11-15: the current period opens with ← enabled even when data
    /// starts inside it; ← stops at the period holding the oldest data (but
    /// never before one step back); → only stops at the current period.
    #[test]
    fn review_navigation_bounds() {
        let t = date_idx(NaiveDate::from_ymd_opt(2026, 11, 15).unwrap());
        let d = |i: i32| { let mut x = Day { idx: i, ..Default::default() }; x.tasks = Some(1.0); x };
        // Data only since 10 Nov (this week and this month).
        let cx = cx_with((t - 5..=t).map(d).collect(), t);
        for kind in ["week", "month"] {
            let now = review(&cx, kind, 0, 0, &default_streaks());
            assert!(now.in_progress && !now.can_next && now.can_prev, "{kind}: current period, ← allowed");
            let prev = review(&cx, kind, -1, 0, &default_streaks());
            assert!(prev.can_next && !prev.can_prev, "{kind}: one step back is the floor");
            assert!(prev.empty && prev.headline.starts_with("No data"), "{kind}: empty period says so");
        }
        assert_eq!(review(&cx, "month", -1, 0, &default_streaks()).label, "October 2026");
        // Data since 20 Aug: months go back to August, not further.
        let cx = cx_with((t - 87..=t).map(d).collect(), t);
        assert_eq!(earliest_offset(&cx, "month", 0), -3);
        assert!(review(&cx, "month", -2, 0, &default_streaks()).can_prev);
        assert!(!review(&cx, "month", -3, 0, &default_streaks()).can_prev);
    }
}
