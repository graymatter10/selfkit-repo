//! Sleep model: estimate a night from device activity, merge it with the
//! manual log, and compute trends, consistency, debt and connections.
//! Pure functions (no I/O) so everything here is unit-tested.
//! Docs: docs/apps/dusk/estimation.md.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

pub type Ms = i64;
pub const MIN: Ms = 60_000;
pub const HOUR: Ms = 60 * MIN;

/// Night window: [previous day WINDOW_PIVOT_H:00, night day WINDOW_PIVOT_H:00) local.
pub const WINDOW_PIVOT_H: u32 = 18;
/// Quiet stretches shorter than this are not sleep candidates.
pub const MIN_QUIET: Ms = 20 * MIN;
/// Activity between two quiet stretches that lasts at most this long is a
/// short wake-up inside one sleep (checking the time, a bathroom trip).
pub const MAX_INTERRUPTION: Ms = 20 * MIN;
/// Only quiet stretches at least this long are joined across a wake-up, so
/// the short pauses of a normal morning are not mistaken for more sleep.
pub const MIN_CHAIN_QUIET: Ms = 60 * MIN;
/// The main sleep needs at least this much quiet time.
pub const MIN_SLEEP: Ms = 150 * MIN;
/// Screen-on periods closer than this are merged into one.
pub const MERGE_GAP: Ms = 2 * MIN;

/// One stretch of device use (any device), UTC ms + local offset (minutes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: Ms,
    pub end: Ms,
    pub tz: i32,
}

/// A row of `sleep_log`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LogEntry {
    #[serde(default)]
    pub id: String,
    pub night: String,
    #[serde(default = "main_kind")]
    pub kind: String,
    #[serde(default)]
    pub bed: Option<Ms>,
    #[serde(default)]
    pub wake: Option<Ms>,
    #[serde(default)]
    pub tz_offset: i32,
    #[serde(default)]
    pub quality: Option<u8>,
    /// Note written in the evening ("Before bed").
    #[serde(default)]
    pub note_before: Option<String>,
    /// Note written in the morning ("After waking").
    #[serde(default)]
    pub note_after: Option<String>,
    #[serde(default)]
    pub created_at: Ms,
}

fn main_kind() -> String {
    "main".into()
}

impl LogEntry {
    pub fn is_nap(&self) -> bool {
        self.kind == "nap"
    }
}

/// A finished sleep session from an outside source (extension point for
/// Android Health Connect / smartwatches; see `health.rs`). It ranks above
/// the device-use estimate and below the manual log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalSleep {
    pub start: Ms,
    pub end: Ms,
    pub tz: i32,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// A main sleep was found.
    Found,
    /// Device use all through the night and no long quiet stretch.
    Allnighter,
    /// The window is not over yet and no sleep has been found so far.
    Ongoing,
    /// Not enough data to say anything.
    NoData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Estimate {
    pub status: Status,
    pub bed: Option<Ms>,
    pub wake: Option<Ms>,
    /// Minutes awake inside [bed, wake] (short wake-ups).
    pub awake_min: i64,
    pub interruptions: u32,
    /// Device use after local midnight until wake (or 06:00), minutes.
    pub late_use_min: i64,
    /// Activity clipped to the window, for the timeline.
    pub activity: Vec<(Ms, Ms)>,
    /// Where the estimate came from: "devices" or an external source name.
    pub source: String,
}

impl Estimate {
    fn empty(status: Status) -> Self {
        Self { status, bed: None, wake: None, awake_min: 0, interruptions: 0, late_use_min: 0, activity: vec![], source: "devices".into() }
    }
    pub fn asleep_min(&self) -> i64 {
        match (self.bed, self.wake) {
            (Some(b), Some(w)) if w > b => (w - b) / MIN - self.awake_min,
            _ => 0,
        }
    }
}

/// One night as shown in the app: estimate, correction and the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NightView {
    pub night: String,
    pub window: (Ms, Ms),
    pub tz_offset: i32,
    pub estimate: Estimate,
    pub log: Option<LogEntry>,
    pub naps: Vec<LogEntry>,
    /// Effective bedtime / wake time (log beats estimate).
    pub bed: Option<Ms>,
    pub wake: Option<Ms>,
    pub asleep_min: i64,
    pub nap_min: i64,
    pub quality: Option<u8>,
    /// "logged" | "estimate" | "allnighter" | "ongoing" | "none"
    pub source: String,
    pub late_use_min: i64,
}

impl NightView {
    pub fn has_sleep(&self) -> bool {
        matches!(self.source.as_str(), "logged" | "estimate" | "allnighter")
    }
    pub fn total_min(&self) -> i64 {
        self.asleep_min + self.nap_min
    }
}

// ---- time helpers ------------------------------------------------------

pub fn date_str(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// UTC ms of a local wall-clock time with a fixed offset.
pub fn local_to_utc(d: NaiveDate, t: NaiveTime, tz: i32) -> Ms {
    d.and_time(t).and_utc().timestamp_millis() - tz as i64 * MIN
}

pub fn local_dt(ms: Ms, tz: i32) -> chrono::NaiveDateTime {
    DateTime::from_timestamp_millis(ms + tz as i64 * MIN).unwrap_or_default().naive_utc()
}

/// Minutes after local midnight.
pub fn local_minute(ms: Ms, tz: i32) -> i64 {
    let t = local_dt(ms, tz);
    (t.hour() * 60 + t.minute()) as i64
}

/// The logical day (shared day-start setting, minutes after midnight) of a moment.
pub fn logical_day(ms: Ms, tz: i32, day_start_min: u16) -> NaiveDate {
    local_dt(ms - day_start_min as i64 * MIN, tz).date()
}

/// Search window of a night (UTC ms).
pub fn window(night: NaiveDate, tz: i32) -> (Ms, Ms) {
    let pivot = NaiveTime::from_hms_opt(WINDOW_PIVOT_H, 0, 0).unwrap();
    (local_to_utc(night - Duration::days(1), pivot, tz), local_to_utc(night, pivot, tz))
}

/// The night a moment belongs to (the date of the morning that ends it).
pub fn night_of(ms: Ms, tz: i32) -> NaiveDate {
    let t = local_dt(ms, tz);
    if t.hour() >= WINDOW_PIVOT_H { t.date() + Duration::days(1) } else { t.date() }
}

/// The night shown as "last night" at `now`: the night ending in the current
/// logical day. Before the day-start hour, that is still yesterday's.
pub fn current_night(now: Ms, tz: i32, day_start_min: u16) -> NaiveDate {
    logical_day(now, tz, day_start_min)
}

/// Put a local "HH:MM" on the right calendar day of a night: evening times
/// (≥ pivot) fall on the day before, others on the night's day.
pub fn time_in_night(night: NaiveDate, hhmm: &str, tz: i32) -> Option<Ms> {
    let t = parse_hhmm(hhmm)?;
    let d = if t.hour() >= WINDOW_PIVOT_H { night - Duration::days(1) } else { night };
    Some(local_to_utc(d, t, tz))
}

/// Accepts "23:30", "2330", "930", "9", "11:30pm", "7a".
pub fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    let s = s.trim().to_ascii_lowercase().replace(' ', "");
    let (body, pm, am) = if let Some(b) = s.strip_suffix("pm").or_else(|| s.strip_suffix('p')) {
        (b.to_string(), true, false)
    } else if let Some(b) = s.strip_suffix("am").or_else(|| s.strip_suffix('a')) {
        (b.to_string(), false, true)
    } else {
        (s.clone(), false, false)
    };
    let (h, m): (u32, u32) = if let Some((h, m)) = body.split_once([':', '.']) {
        (h.parse().ok()?, m.parse().ok()?)
    } else {
        if body.is_empty() || !body.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        match body.len() {
            1 | 2 => (body.parse().ok()?, 0),
            3 => (body[..1].parse().ok()?, body[1..].parse().ok()?),
            4 => (body[..2].parse().ok()?, body[2..].parse().ok()?),
            _ => return None,
        }
    };
    let h = match (pm, am, h) {
        (true, _, 1..=11) => h + 12,
        (_, true, 12) => 0,
        (_, _, h) if (pm || am) && h > 12 => return None,
        _ => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

// ---- spans -------------------------------------------------------------

/// Sort and union spans from all devices; spans closer than MERGE_GAP join.
pub fn merge_spans(mut v: Vec<Span>) -> Vec<Span> {
    v.retain(|s| s.end > s.start);
    v.sort_by_key(|s| s.start);
    let mut out: Vec<Span> = Vec::with_capacity(v.len());
    for s in v {
        match out.last_mut() {
            Some(l) if s.start <= l.end + MERGE_GAP => l.end = l.end.max(s.end),
            _ => out.push(s),
        }
    }
    out
}

/// Weight of quiet time by local hour: night hours count fully, daytime less,
/// so a long afternoon away from screens doesn't beat a real night.
fn weighted(start: Ms, end: Ms, tz: i32) -> f64 {
    let step = 5 * MIN;
    let mut t = start;
    let mut w = 0.0;
    while t < end {
        let n = (end - t).min(step);
        let h = local_dt(t, tz).hour();
        let k = if !(11..21).contains(&h) { 1.0 } else { 0.4 };
        w += k * n as f64;
        t += n;
    }
    w
}

/// Estimate one night from merged spans.
/// `now_active`: the user is using this device right now (the app is open),
/// so "now" counts as activity: opening Dusk in the morning ends the sleep.
pub fn estimate(spans: &[Span], night: NaiveDate, tz: i32, now: Ms, now_active: bool) -> Estimate {
    let (ws, we) = window(night, tz);
    let end = we.min(now);
    if end <= ws {
        return Estimate::empty(Status::NoData);
    }
    let mut acts: Vec<(Ms, Ms)> = spans
        .iter()
        .filter(|s| s.end > ws && s.start < end)
        .map(|s| (s.start.max(ws), s.end.min(end)))
        .filter(|(a, b)| b > a)
        .collect();
    if now_active && now >= ws && now < we {
        acts.push(((now - MIN).max(ws), now));
        acts.sort();
        let mut m: Vec<(Ms, Ms)> = Vec::new();
        for a in acts {
            match m.last_mut() {
                Some(l) if a.0 <= l.1 + MERGE_GAP => l.1 = l.1.max(a.1),
                _ => m.push(a),
            }
        }
        acts = m;
    }
    if acts.is_empty() {
        return Estimate::empty(if now < we { Status::Ongoing } else { Status::NoData });
    }
    // Quiet stretches between activity, chained across short wake-ups: a
    // chain is a run of quiet stretches of at least MIN_CHAIN_QUIET whose
    // separating activity lasts at most MAX_INTERRUPTION. Shorter quiet
    // stretches (20–60 min) only count on their own.
    let gaps: Vec<(Ms, Ms)> = acts.windows(2).map(|w| (w[0].1, w[1].0)).filter(|(a, b)| b - a >= MIN_QUIET).collect();
    let long = |g: &(Ms, Ms)| g.1 - g.0 >= MIN_CHAIN_QUIET;
    let mut best: Option<(f64, usize, usize)> = None;
    let mut i = 0;
    while i < gaps.len() {
        let mut j = i;
        while long(&gaps[j]) && j + 1 < gaps.len() && long(&gaps[j + 1]) && gaps[j + 1].0 - gaps[j].1 <= MAX_INTERRUPTION {
            j += 1;
        }
        let quiet: Ms = gaps[i..=j].iter().map(|g| g.1 - g.0).sum();
        if quiet >= MIN_SLEEP {
            let score: f64 = gaps[i..=j].iter().map(|g| weighted(g.0, g.1, tz)).sum();
            if best.is_none_or(|b| score > b.0) {
                best = Some((score, i, j));
            }
        }
        i = j + 1;
    }
    let local = |h: u32| local_to_utc(night, NaiveTime::from_hms_opt(h, 0, 0).unwrap(), tz);
    let midnight = local(0);
    let late = |until: Ms| -> i64 { acts.iter().map(|a| (a.1.min(until) - a.0.max(midnight)).max(0)).sum::<Ms>() / MIN };
    if let Some((_, i, j)) = best {
        let bed = gaps[i].0;
        let wake = gaps[j].1;
        let quiet: Ms = gaps[i..=j].iter().map(|g| g.1 - g.0).sum();
        return Estimate {
            status: Status::Found,
            bed: Some(bed),
            wake: Some(wake),
            awake_min: ((wake - bed) - quiet) / MIN,
            interruptions: (j - i) as u32,
            late_use_min: late(wake),
            activity: acts,
            source: "devices".into(),
        };
    }
    let used_core = acts.iter().any(|a| a.1 > local(1) && a.0 < local(5));
    let status = if end >= local(6) && used_core {
        Status::Allnighter
    } else if now < we {
        Status::Ongoing
    } else {
        Status::NoData
    };
    Estimate { status, late_use_min: late(local(6)), activity: acts, ..Estimate::empty(status) }
}

/// Outside sleep sessions (Health Connect) beat the device estimate.
fn apply_external(mut est: Estimate, ext: &[ExternalSleep], ws: Ms, we: Ms) -> Estimate {
    let Some(x) = ext.iter().filter(|x| x.end > ws && x.start < we).max_by_key(|x| x.end - x.start) else { return est };
    est.status = Status::Found;
    est.bed = Some(x.start);
    est.wake = Some(x.end);
    est.awake_min = 0;
    est.interruptions = 0;
    est.source = x.source.clone();
    est
}

/// Choose the offset for a night: the most common offset of activity near
/// it, else the fallback (the device's current offset).
fn night_tz(spans: &[Span], night: NaiveDate, fallback: i32, log: Option<&LogEntry>) -> i32 {
    if let Some(l) = log.filter(|l| l.bed.is_some() || l.wake.is_some()) {
        return l.tz_offset;
    }
    let (ws, we) = window(night, fallback);
    let mut count: BTreeMap<i32, usize> = BTreeMap::new();
    for s in spans.iter().filter(|s| s.end > ws && s.start < we) {
        *count.entry(s.tz).or_default() += 1;
    }
    count.into_iter().max_by_key(|(_, n)| *n).map(|(tz, _)| tz).unwrap_or(fallback)
}

pub struct Inputs<'a> {
    /// Merged, sorted (see `merge_spans`).
    pub spans: &'a [Span],
    pub logs: &'a [LogEntry],
    pub external: &'a [ExternalSleep],
    pub now: Ms,
    pub now_active: bool,
    pub fallback_tz: i32,
}

/// Build one night: estimate, then apply the manual log.
pub fn night_view(inp: &Inputs, night: NaiveDate) -> NightView {
    let key = date_str(night);
    let log = inp.logs.iter().find(|l| l.night == key && !l.is_nap()).cloned();
    let naps: Vec<LogEntry> = inp.logs.iter().filter(|l| l.night == key && l.is_nap()).cloned().collect();
    let tz = night_tz(inp.spans, night, inp.fallback_tz, log.as_ref());
    let (ws, we) = window(night, tz);
    let est = apply_external(estimate(inp.spans, night, tz, inp.now, inp.now_active), inp.external, ws, we);
    let found = est.status == Status::Found;
    let lb = log.as_ref().and_then(|l| l.bed);
    let lw = log.as_ref().and_then(|l| l.wake);
    let bed = lb.or(est.bed);
    let wake = lw.or(est.wake);
    let logged = lb.is_some() || lw.is_some();
    let (asleep_min, source) = match (bed, wake) {
        (Some(b), Some(w)) if w > b => {
            // Short wake-ups only count when the estimate's span is kept as is.
            let awake = if !logged && found { est.awake_min } else { 0 };
            ((w - b) / MIN - awake, if logged { "logged" } else { "estimate" })
        }
        _ if est.status == Status::Allnighter => (0, "allnighter"),
        _ if est.status == Status::Ongoing => (0, "ongoing"),
        _ => (0, "none"),
    };
    let nap_min = naps.iter().filter_map(|n| Some((n.wake? - n.bed?) / MIN)).filter(|m| *m > 0).sum();
    NightView {
        night: key,
        window: (ws, we),
        tz_offset: tz,
        late_use_min: est.late_use_min,
        quality: log.as_ref().and_then(|l| l.quality),
        estimate: est,
        log,
        naps,
        bed,
        wake,
        asleep_min,
        nap_min,
        source: source.into(),
    }
}

/// Nights from `from` to `to` inclusive, oldest first.
pub fn nights(inp: &Inputs, from: NaiveDate, to: NaiveDate) -> Vec<NightView> {
    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        out.push(night_view(inp, d));
        d += Duration::days(1);
    }
    out
}

// ---- stats -------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Goals {
    pub goal_min: i64,
    /// Bedtime goal, minutes after local midnight (e.g. 1380 = 23:00).
    pub bedtime_min: i64,
    pub winddown_min: i64,
}

impl Default for Goals {
    fn default() -> Self {
        Self { goal_min: 480, bedtime_min: 23 * 60, winddown_min: 30 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Stats {
    /// Nights with sleep data in the period.
    pub nights: usize,
    pub avg_min: Option<i64>,
    /// Average over the period before (for the trend arrow).
    pub prev_avg_min: Option<i64>,
    /// 0–100, from the spread of bed and wake times. None with < 3 nights.
    pub consistency: Option<i64>,
    pub bed_sd_min: Option<i64>,
    pub wake_sd_min: Option<i64>,
    /// Hours short of the goal over the period (surplus offsets, never < 0).
    pub debt_min: i64,
    pub avg_bed_min: Option<i64>,
    pub avg_wake_min: Option<i64>,
    /// Nights in bed by the bedtime goal (+15 min grace), of nights with a bedtime.
    pub bedtime_hits: usize,
    pub bedtime_total: usize,
    pub avg_quality: Option<f64>,
}

/// Bedtime as minutes from local midnight, evening negative (23:30 → -30).
pub fn bed_rel(ms: Ms, tz: i32) -> i64 {
    let m = local_minute(ms, tz);
    if m >= WINDOW_PIVOT_H as i64 * 60 { m - 1440 } else { m }
}

fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

fn sd(v: &[f64]) -> Option<f64> {
    let m = mean(v)?;
    (v.len() >= 2).then(|| (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt())
}

/// Stats over the last `days` nights of `ns` (oldest first).
pub fn stats(ns: &[NightView], days: usize, goals: &Goals) -> Stats {
    let tail = &ns[ns.len().saturating_sub(days)..];
    let before = &ns[ns.len().saturating_sub(days * 2)..ns.len().saturating_sub(days)];
    let with: Vec<&NightView> = tail.iter().filter(|n| n.has_sleep()).collect();
    let avg = |v: &[&NightView]| mean(&v.iter().map(|n| n.total_min() as f64).collect::<Vec<_>>()).map(|x| x.round() as i64);
    let prev: Vec<&NightView> = before.iter().filter(|n| n.has_sleep()).collect();
    let beds: Vec<f64> = with.iter().filter_map(|n| Some(bed_rel(n.bed?, n.tz_offset) as f64)).collect();
    let wakes: Vec<f64> = with.iter().filter_map(|n| Some(local_minute(n.wake?, n.tz_offset) as f64)).collect();
    let (bsd, wsd) = (sd(&beds), sd(&wakes));
    let consistency = match (bsd, wsd) {
        (Some(b), Some(w)) if beds.len() >= 3 && wakes.len() >= 3 => Some((100.0 - (b + w) / 2.0 / 0.9).clamp(0.0, 100.0).round() as i64),
        _ => None,
    };
    let debt = with.iter().map(|n| goals.goal_min - n.total_min()).sum::<i64>().max(0);
    let goal_rel = if goals.bedtime_min >= WINDOW_PIVOT_H as i64 * 60 { goals.bedtime_min - 1440 } else { goals.bedtime_min };
    let hits = beds.iter().filter(|b| **b <= (goal_rel + 15) as f64).count();
    let q: Vec<f64> = tail.iter().filter_map(|n| n.quality.map(f64::from)).collect();
    Stats {
        nights: with.len(),
        avg_min: avg(&with),
        prev_avg_min: avg(&prev),
        consistency,
        bed_sd_min: bsd.map(|x| x.round() as i64),
        wake_sd_min: wsd.map(|x| x.round() as i64),
        debt_min: debt,
        avg_bed_min: mean(&beds).map(|x| x.round() as i64),
        avg_wake_min: mean(&wakes).map(|x| x.round() as i64),
        bedtime_hits: hits,
        bedtime_total: beds.len(),
        avg_quality: mean(&q).map(|x| (x * 10.0).round() / 10.0),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeekAvg {
    /// Monday of the week (night dates).
    pub week: String,
    pub avg_min: i64,
    pub nights: usize,
}

pub fn weekly(ns: &[NightView]) -> Vec<WeekAvg> {
    let mut by: BTreeMap<NaiveDate, Vec<i64>> = BTreeMap::new();
    for n in ns.iter().filter(|n| n.has_sleep()) {
        let Some(d) = parse_date(&n.night) else { continue };
        let monday = d - Duration::days(chrono::Datelike::weekday(&d).num_days_from_monday() as i64);
        by.entry(monday).or_default().push(n.total_min());
    }
    by.into_iter()
        .map(|(w, v)| WeekAvg { week: date_str(w), avg_min: v.iter().sum::<i64>() / v.len() as i64, nights: v.len() })
        .collect()
}

// ---- connections -------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bucket {
    pub label: String,
    pub nights: usize,
    /// Average of the measured value (quality, tasks, money) in this bucket.
    pub avg: Option<f64>,
    /// Average sleep in this bucket, minutes.
    pub avg_sleep_min: Option<i64>,
}

/// One connection card: a plain finding, two compared groups ("a" is the
/// highlighted one), and the 4-group breakdown + correlation behind "Details".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    /// "Late screen use & sleep quality"
    pub topic: String,
    /// One plain sentence, or a hint when there is not enough data yet.
    pub finding: String,
    /// 0 no clear link · 1 weak · 2 moderate · 3 strong (from |r|).
    pub strength: u8,
    /// The two compared groups: [highlighted, other].
    pub groups: Vec<Bucket>,
    /// Breakdown into 4 groups (Details).
    pub buckets: Vec<Bucket>,
    /// Pearson r between the two series (None with < 5 pairs or no variance).
    pub r: Option<f64>,
    pub pairs: usize,
    /// "nights" or "days"
    pub per: String,
    /// Unit shown after values: "of 5", "tasks", or a currency symbol prefix.
    pub unit: String,
    /// Where the data comes from: "Chrono", "Agenda", "Hisab", "this phone".
    pub src: String,
    /// What is averaged in Details ("Average rating").
    pub measure: String,
    /// False when the data source is missing (e.g. no Hisab on this device).
    pub available: bool,
    /// Enough data for a finding (≥ 3 in each group and r computed).
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

fn bucket(label: &str, inb: &[&(f64, f64, i64)]) -> Bucket {
    Bucket {
        label: label.into(),
        nights: inb.len(),
        avg: mean(&inb.iter().map(|p| p.1).collect::<Vec<_>>()).map(|x| (x * 10.0).round() / 10.0),
        avg_sleep_min: mean(&inb.iter().map(|p| p.2 as f64).collect::<Vec<_>>()).map(|x| x.round() as i64),
    }
}

fn bucketize(pairs: &[(f64, f64, i64)], edges: &[(f64, f64, &str)]) -> Vec<Bucket> {
    edges
        .iter()
        .map(|(lo, hi, label)| bucket(label, &pairs.iter().filter(|p| p.0 >= *lo && p.0 < *hi).collect::<Vec<_>>()))
        .collect()
}

pub fn strength(r: Option<f64>) -> u8 {
    match r.map(f64::abs) {
        Some(x) if x >= 0.5 => 3,
        Some(x) if x >= 0.3 => 2,
        Some(x) if x >= 0.15 => 1,
        _ => 0,
    }
}

struct Spec<'a> {
    id: &'a str,
    topic: &'a str,
    edges: &'a [(f64, f64, &'a str)],
    split: f64,
    /// Labels: [x ≥ split, x < split] or the other way (see `hi_first`).
    labels: [&'a str; 2],
    /// Group "a" (highlighted) is x ≥ split when true.
    a_high: bool,
    per: &'a str,
    unit: &'a str,
    src: &'a str,
    measure: &'a str,
    available: bool,
}

fn connect(pairs: Vec<(f64, f64, i64)>, sp: Spec, finding: impl Fn(f64, f64, u8) -> String) -> Connection {
    let r = pearson(&pairs.iter().map(|p| p.0).collect::<Vec<_>>(), &pairs.iter().map(|p| p.1).collect::<Vec<_>>());
    let hi: Vec<&(f64, f64, i64)> = pairs.iter().filter(|p| p.0 >= sp.split).collect();
    let lo: Vec<&(f64, f64, i64)> = pairs.iter().filter(|p| p.0 < sp.split).collect();
    let (ga, gb) = if sp.a_high { (&hi, &lo) } else { (&lo, &hi) };
    let groups = vec![bucket(sp.labels[0], ga), bucket(sp.labels[1], gb)];
    let enough = sp.available && r.is_some() && ga.len() >= 3 && gb.len() >= 3;
    let st = if enough { strength(r) } else { 0 };
    let text = if !sp.available {
        format!("Shows up where Dusk can read {} (on your computer).", sp.src)
    } else if !enough {
        format!("Not enough data yet ({} so far).", count(pairs.len(), sp.per))
    } else {
        finding(groups[0].avg.unwrap_or(0.0), groups[1].avg.unwrap_or(0.0), st)
    };
    Connection {
        id: sp.id.into(),
        topic: sp.topic.into(),
        finding: text,
        strength: st,
        groups,
        buckets: bucketize(&pairs, sp.edges),
        r,
        pairs: pairs.len(),
        per: sp.per.into(),
        unit: sp.unit.into(),
        src: sp.src.into(),
        measure: sp.measure.into(),
        available: sp.available,
        enough,
    }
}

/// "1 rated night", "3 days".
pub fn count(n: usize, per: &str) -> String {
    if n == 1 { format!("1 {}", per.strip_suffix('s').unwrap_or(per)) } else { format!("{n} {per}") }
}

fn may(st: u8) -> &'static str {
    if st <= 1 { "may " } else { "" }
}

/// Screen time after midnight against sleep quality (rated nights).
pub fn late_use_vs_quality(ns: &[NightView], src: &str) -> Connection {
    let pairs: Vec<(f64, f64, i64)> =
        ns.iter().filter(|n| n.has_sleep()).filter_map(|n| Some((n.late_use_min as f64, n.quality? as f64, n.total_min()))).collect();
    let sp = Spec {
        id: "late_use",
        topic: "Late screen use & sleep quality",
        edges: &[(0.0, 1.0, "No screen after midnight"), (1.0, 30.0, "Under 30 min"), (30.0, 60.0, "30–60 min"), (60.0, f64::MAX, "Over 1 hour")],
        split: 30.0,
        labels: ["30+ min on a screen after midnight", "Other nights"],
        a_high: true,
        per: "rated nights",
        unit: "of 5",
        src,
        measure: "Average rating",
        available: true,
    };
    connect(pairs, sp, |a, b, st| {
        if st == 0 || (a - b).abs() < 0.2 {
            "Late screen use makes little difference to your sleep.".into()
        } else if a < b {
            format!("You {}sleep worse after late screen use.", may(st))
        } else {
            format!("You {}sleep better after late screen use.", may(st))
        }
    })
}

const SLEEP_EDGES: &[(f64, f64, &str)] = &[(0.0, 360.0, "Under 6 h"), (360.0, 420.0, "6 to 7 h"), (420.0, 480.0, "7 to 8 h"), (480.0, f64::MAX, "8 h or more")];

/// Sleep against a per-day value of the day after waking (tasks done, money spent).
/// `per_day`: logical date → value; days missing from the map count as 0
/// only inside `[first_day, last_day]` of the source.
pub fn sleep_vs_next_day(ns: &[NightView], per_day: &BTreeMap<NaiveDate, f64>, day_start_min: u16, kind: &str, unit: &str, available: bool) -> Connection {
    let (first, last) = (per_day.keys().next().copied(), per_day.keys().next_back().copied());
    let pairs: Vec<(f64, f64, i64)> = ns
        .iter()
        .filter(|n| n.has_sleep())
        .filter_map(|n| {
            let day = match n.wake {
                Some(w) => logical_day(w, n.tz_offset, day_start_min),
                None => parse_date(&n.night)?,
            };
            if first.is_none_or(|f| day < f) || last.is_none_or(|l| day > l) {
                return None;
            }
            Some((n.asleep_min as f64, per_day.get(&day).copied().unwrap_or(0.0), n.total_min()))
        })
        .collect();
    let tasks = kind == "tasks";
    let sp = Spec {
        id: kind,
        topic: if tasks { "Sleep & next-day tasks" } else { "Sleep & next-day spending" },
        edges: SLEEP_EDGES,
        split: 420.0,
        labels: ["After less than 7 h", "After 7 h or more"],
        a_high: false,
        per: "days",
        unit,
        src: if tasks { "Agenda" } else { "Hisab" },
        measure: if tasks { "Tasks done next day" } else { "Spent next day" },
        available,
    };
    connect(pairs, sp, move |short, long, st| {
        let small = (short - long).abs() < 0.05 * short.max(long).max(1.0);
        match (tasks, st == 0 || small, long > short) {
            (true, true, _) => "Your sleep makes little difference to how much you get done.".into(),
            (true, false, true) => format!("You {}get more done after 7 h+ of sleep.", may(st)),
            (true, false, false) => format!("You {}get more done after short nights.", may(st)),
            (false, true, _) => "Your sleep makes little difference to what you spend.".into(),
            (false, false, false) => format!("You {}spend more after short nights.", may(st)),
            (false, false, true) => format!("You {}spend more after 7 h+ of sleep.", may(st)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TZ: i32 = 330; // UTC+5:30, a non-whole-hour offset on purpose

    fn d(s: &str) -> NaiveDate {
        parse_date(s).unwrap()
    }
    fn at(date: &str, hhmm: &str) -> Ms {
        local_to_utc(d(date), parse_hhmm(hhmm).unwrap(), TZ)
    }
    fn span(a: Ms, b: Ms) -> Span {
        Span { start: a, end: b, tz: TZ }
    }
    fn day_use(spans: &mut Vec<Span>, date: &str, from: &str, to: &str) {
        // A screen session every 10 minutes for 5 minutes.
        let (mut t, e) = (at(date, from), at(date, to));
        while t < e {
            spans.push(span(t, (t + 5 * MIN).min(e)));
            t += 10 * MIN;
        }
    }

    #[test]
    fn parses_times() {
        let t = |s: &str| parse_hhmm(s).map(|t| t.format("%H:%M").to_string());
        assert_eq!(t("23:30").as_deref(), Some("23:30"));
        assert_eq!(t("2330").as_deref(), Some("23:30"));
        assert_eq!(t("930").as_deref(), Some("09:30"));
        assert_eq!(t("7").as_deref(), Some("07:00"));
        assert_eq!(t("11:30pm").as_deref(), Some("23:30"));
        assert_eq!(t("12a").as_deref(), Some("00:00"));
        assert_eq!(t("7.15").as_deref(), Some("07:15"));
        assert_eq!(t("25:00"), None);
        assert_eq!(t("abc"), None);
        assert_eq!(t("13pm"), None);
    }

    #[test]
    fn windows_and_nights() {
        let n = d("2026-03-10");
        let (ws, we) = window(n, TZ);
        assert_eq!(ws, at("2026-03-09", "18:00"));
        assert_eq!(we, at("2026-03-10", "18:00"));
        assert_eq!(night_of(at("2026-03-09", "23:00"), TZ), n);
        assert_eq!(night_of(at("2026-03-10", "07:00"), TZ), n);
        assert_eq!(time_in_night(n, "23:15", TZ), Some(at("2026-03-09", "23:15")));
        assert_eq!(time_in_night(n, "01:15", TZ), Some(at("2026-03-10", "01:15")));
    }

    #[test]
    fn day_start_moves_last_night() {
        let now = at("2026-03-10", "02:00");
        assert_eq!(current_night(now, TZ, 0), d("2026-03-10"));
        assert_eq!(current_night(now, TZ, 240), d("2026-03-09"));
        assert_eq!(logical_day(at("2026-03-10", "03:00"), TZ, 240), d("2026-03-09"));
    }

    #[test]
    fn finds_main_sleep_with_wakeup() {
        let mut s = vec![];
        day_use(&mut s, "2026-03-09", "19:00", "23:40");
        s.push(span(at("2026-03-10", "03:10"), at("2026-03-10", "03:14"))); // checks the phone
        day_use(&mut s, "2026-03-10", "07:05", "09:00");
        let s = merge_spans(s);
        let e = estimate(&s, d("2026-03-10"), TZ, at("2026-03-11", "12:00"), false);
        assert_eq!(e.status, Status::Found);
        assert_eq!(e.bed, Some(at("2026-03-09", "23:35")));
        assert_eq!(e.wake, Some(at("2026-03-10", "07:05")));
        assert_eq!(e.interruptions, 1);
        assert_eq!(e.awake_min, 4);
        assert_eq!(e.asleep_min(), 7 * 60 + 30 - 4);
        assert_eq!(e.late_use_min, 4);
    }

    #[test]
    fn morning_pauses_do_not_extend_sleep() {
        let mut s = vec![];
        day_use(&mut s, "2026-03-09", "19:00", "23:30");
        // after waking: 5 min use, 40 min pause, 5 min, 40 min pause …
        for (a, b) in [("07:00", "07:05"), ("07:45", "07:50"), ("08:30", "08:35"), ("09:15", "09:20")] {
            s.push(span(at("2026-03-10", a), at("2026-03-10", b)));
        }
        let e = estimate(&merge_spans(s), d("2026-03-10"), TZ, at("2026-03-11", "00:00"), false);
        assert_eq!(e.wake, Some(at("2026-03-10", "07:00")));
        assert_eq!(e.interruptions, 0);
    }

    #[test]
    fn long_wakeup_splits_and_longest_night_part_wins() {
        let mut s = vec![];
        day_use(&mut s, "2026-03-09", "20:00", "22:00");
        day_use(&mut s, "2026-03-10", "01:00", "02:00"); // up for an hour
        day_use(&mut s, "2026-03-10", "08:00", "10:00");
        let e = estimate(&merge_spans(s), d("2026-03-10"), TZ, at("2026-03-11", "00:00"), false);
        assert_eq!(e.bed, Some(at("2026-03-10", "01:55")));
        assert_eq!(e.wake, Some(at("2026-03-10", "08:00")));
        assert!(e.late_use_min >= 30);
    }

    #[test]
    fn night_beats_afternoon_quiet() {
        let mut s = vec![];
        day_use(&mut s, "2026-03-09", "18:00", "23:00");
        // 01:00–05:00 quiet (4 h, night) vs 11:00–16:00 quiet (5 h, day)
        day_use(&mut s, "2026-03-10", "00:00", "01:00");
        day_use(&mut s, "2026-03-10", "05:00", "11:00");
        day_use(&mut s, "2026-03-10", "16:00", "17:59");
        let e = estimate(&merge_spans(s), d("2026-03-10"), TZ, at("2026-03-11", "00:00"), false);
        assert_eq!(e.wake, Some(at("2026-03-10", "05:00")));
    }

    #[test]
    fn allnighter_and_nodata_and_ongoing() {
        let mut s = vec![];
        day_use(&mut s, "2026-03-09", "18:00", "23:59");
        day_use(&mut s, "2026-03-10", "00:00", "17:59");
        let e = estimate(&merge_spans(s), d("2026-03-10"), TZ, at("2026-03-11", "00:00"), false);
        assert_eq!(e.status, Status::Allnighter);
        assert!(e.late_use_min > 100);

        let e = estimate(&[], d("2026-03-10"), TZ, at("2026-03-11", "00:00"), false);
        assert_eq!(e.status, Status::NoData);

        let mut s = vec![];
        day_use(&mut s, "2026-03-09", "19:00", "23:00");
        let s = merge_spans(s);
        // 23:30, still the same night, nothing found yet
        let e = estimate(&s, d("2026-03-10"), TZ, at("2026-03-09", "23:30"), false);
        assert_eq!(e.status, Status::Ongoing);
        // opening the app at 07:00 ends the sleep
        let e = estimate(&s, d("2026-03-10"), TZ, at("2026-03-10", "07:00"), true);
        assert_eq!(e.status, Status::Found);
        assert_eq!(e.wake, Some(at("2026-03-10", "06:59")));
    }

    #[test]
    fn merges_devices() {
        // phone until 00:30, laptop from 06:40: one sleep across devices
        let s = merge_spans(vec![
            span(at("2026-03-09", "22:00"), at("2026-03-10", "00:30")),
            span(at("2026-03-09", "21:00"), at("2026-03-09", "23:00")),
            span(at("2026-03-10", "06:40"), at("2026-03-10", "08:00")),
        ]);
        assert_eq!(s.len(), 2);
        let e = estimate(&s, d("2026-03-10"), TZ, at("2026-03-11", "00:00"), false);
        assert_eq!((e.bed, e.wake), (Some(at("2026-03-10", "00:30")), Some(at("2026-03-10", "06:40"))));
    }

    fn inputs<'a>(s: &'a [Span], logs: &'a [LogEntry], now: Ms) -> Inputs<'a> {
        Inputs { spans: s, logs, external: &[], now, now_active: false, fallback_tz: TZ }
    }

    #[test]
    fn log_overrides_estimate_and_keeps_it_visible() {
        let mut s = vec![];
        day_use(&mut s, "2026-03-09", "19:00", "23:40");
        day_use(&mut s, "2026-03-10", "07:05", "09:00");
        let s = merge_spans(s);
        let rate_only = vec![LogEntry { night: "2026-03-10".into(), kind: "main".into(), quality: Some(4), tz_offset: TZ, ..Default::default() }];
        let v = night_view(&inputs(&s, &rate_only, at("2026-03-12", "00:00")), d("2026-03-10"));
        assert_eq!((v.source.as_str(), v.quality), ("estimate", Some(4)));
        assert_eq!(v.asleep_min, 7 * 60 + 30);

        let fixed = vec![LogEntry {
            night: "2026-03-10".into(),
            kind: "main".into(),
            bed: Some(at("2026-03-09", "23:00")),
            wake: None,
            quality: Some(2),
            tz_offset: TZ,
            ..Default::default()
        }];
        let v = night_view(&inputs(&s, &fixed, at("2026-03-12", "00:00")), d("2026-03-10"));
        assert_eq!(v.source, "logged");
        assert_eq!(v.asleep_min, 8 * 60 + 5);
        assert_eq!(v.estimate.bed, Some(at("2026-03-09", "23:35"))); // estimate still there
    }

    #[test]
    fn naps_and_manual_without_data() {
        let logs = vec![
            LogEntry {
                night: "2026-03-10".into(),
                kind: "main".into(),
                bed: Some(at("2026-03-09", "23:00")),
                wake: Some(at("2026-03-10", "06:00")),
                tz_offset: TZ,
                ..Default::default()
            },
            LogEntry {
                id: "x".into(),
                night: "2026-03-10".into(),
                kind: "nap".into(),
                bed: Some(at("2026-03-10", "14:00")),
                wake: Some(at("2026-03-10", "14:40")),
                tz_offset: TZ,
                ..Default::default()
            },
        ];
        let v = night_view(&inputs(&[], &logs, at("2026-03-12", "00:00")), d("2026-03-10"));
        assert_eq!((v.asleep_min, v.nap_min, v.total_min()), (420, 40, 460));
        assert_eq!(v.source, "logged");
    }

    #[test]
    fn external_source_beats_estimate() {
        let mut s = vec![];
        day_use(&mut s, "2026-03-09", "19:00", "23:40");
        day_use(&mut s, "2026-03-10", "07:05", "09:00");
        let s = merge_spans(s);
        let ext = vec![ExternalSleep { start: at("2026-03-09", "23:50"), end: at("2026-03-10", "06:50"), tz: TZ, source: "health_connect".into() }];
        let inp = Inputs { spans: &s, logs: &[], external: &ext, now: at("2026-03-12", "00:00"), now_active: false, fallback_tz: TZ };
        let v = night_view(&inp, d("2026-03-10"));
        assert_eq!(v.asleep_min, 420);
        assert_eq!(v.estimate.source, "health_connect");
    }

    #[test]
    fn timezone_travel_uses_activity_offset() {
        // Activity recorded at UTC+1 while the device is now at UTC+5:30.
        let tz1 = 60;
        let at1 = |date: &str, hm: &str| local_to_utc(d(date), parse_hhmm(hm).unwrap(), tz1);
        let s = merge_spans(vec![
            Span { start: at1("2026-03-09", "20:00"), end: at1("2026-03-09", "23:00"), tz: tz1 },
            Span { start: at1("2026-03-10", "07:00"), end: at1("2026-03-10", "08:00"), tz: tz1 },
        ]);
        let v = night_view(&inputs(&s, &[], at("2026-03-12", "00:00")), d("2026-03-10"));
        assert_eq!(v.tz_offset, tz1);
        assert_eq!(v.asleep_min, 480);
        assert_eq!(bed_rel(v.bed.unwrap(), v.tz_offset), -60);
    }

    fn night_with(date: &str, bed: &str, wake: &str, q: Option<u8>, late: i64) -> NightView {
        let n = d(date);
        let b = time_in_night(n, bed, TZ).unwrap();
        let w = time_in_night(n, wake, TZ).unwrap();
        NightView {
            night: date.into(),
            window: window(n, TZ),
            tz_offset: TZ,
            estimate: Estimate::empty(Status::Found),
            log: None,
            naps: vec![],
            bed: Some(b),
            wake: Some(w),
            asleep_min: (w - b) / MIN,
            nap_min: 0,
            quality: q,
            source: "estimate".into(),
            late_use_min: late,
        }
    }

    #[test]
    fn stats_consistency_debt_goal() {
        let g = Goals::default();
        let same: Vec<NightView> = (1..=7).map(|i| night_with(&format!("2026-03-{i:02}"), "23:00", "07:00", None, 0)).collect();
        let s = stats(&same, 7, &g);
        assert_eq!((s.nights, s.avg_min, s.consistency, s.debt_min), (7, Some(480), Some(100), 0));
        assert_eq!((s.bedtime_hits, s.bedtime_total, s.avg_bed_min), (7, 7, Some(-60)));

        let messy = vec![
            night_with("2026-03-01", "22:00", "06:00", None, 0),
            night_with("2026-03-02", "01:00", "07:00", None, 0),
            night_with("2026-03-03", "23:30", "09:30", None, 0),
            night_with("2026-03-04", "02:00", "07:00", None, 0),
        ];
        let s = stats(&messy, 7, &g);
        assert!(s.consistency.unwrap() < 30, "{:?}", s.consistency);
        // 480+360+600+300 = 1740 vs goal 1920 → 180 min debt
        assert_eq!(s.debt_min, 180);
        assert_eq!(s.bedtime_hits, 1);
    }

    #[test]
    fn connections() {
        let mut ns = vec![];
        for i in 1..=20 {
            let late = if i % 2 == 0 { 70 } else { 0 };
            let q = if late > 0 { 2 } else { 4 };
            let wake = if i % 2 == 0 { "05:30" } else { "07:30" };
            ns.push(night_with(&format!("2026-03-{i:02}"), "23:30", wake, Some(q), late));
        }
        let c = late_use_vs_quality(&ns, "Chrono");
        assert_eq!(c.pairs, 20);
        assert!(c.r.unwrap() < -0.9);
        assert_eq!(c.strength, 3);
        assert_eq!(c.finding, "You sleep worse after late screen use.");
        assert_eq!((c.groups[0].avg, c.groups[0].nights, c.groups[1].avg), (Some(2.0), 10, Some(4.0)));
        assert_eq!(c.buckets[0].avg, Some(4.0));
        assert_eq!(c.buckets[3].avg, Some(2.0));

        let mut tasks = BTreeMap::new();
        for i in 1..=20 {
            tasks.insert(d(&format!("2026-03-{i:02}")), if i % 2 == 0 { 2.0 } else { 5.0 });
        }
        let c = sleep_vs_next_day(&ns, &tasks, 0, "tasks", "tasks", true);
        assert!(c.r.unwrap() > 0.9);
        assert_eq!(c.finding, "You get more done after 7 h+ of sleep.");
        assert_eq!((c.groups[0].avg, c.groups[1].avg), (Some(2.0), Some(5.0)));
        let none = sleep_vs_next_day(&ns, &BTreeMap::new(), 0, "money", "$", false);
        assert_eq!((none.pairs, none.available, none.enough), (0, false, false));
        assert_eq!(none.finding, "Shows up where Dusk can read Hisab (on your computer).");
        let few = late_use_vs_quality(&ns[..4], "Chrono");
        assert!(!few.enough && few.finding.starts_with("Not enough data"));
    }

    #[test]
    fn weekly_averages() {
        let ns: Vec<NightView> = (2..=15).map(|i| night_with(&format!("2026-03-{i:02}"), "23:00", "07:00", None, 0)).collect();
        let w = weekly(&ns);
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].week.as_str(), w[0].nights, w[0].avg_min), ("2026-03-02", 7, 480));
    }
}
