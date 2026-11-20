//! Category resolution, the one place every Chrono screen, goal and alert
//! gets a category from. Order: an enabled time rule that covers the moment
//! (`chrono_settings.category_rules`) > the user's static mapping
//! (`categories`) > built-in defaults > `other`. Also browser detection and
//! Focus. Categories: study, work, social, entertainment, other (or a
//! user-made lowercase name). Reflect and Dusk don't use categories.

use crate::settings::{CategoryRule, ChronoSettings, domain_matches, norm_domain};

const DAY: i64 = 86_400_000;
const MIN: i64 = 60_000;

pub const CATEGORIES: &[&str] = &["study", "work", "social", "entertainment", "other"];
pub const FOCUS: &[&str] = &["study", "work"];
pub const DISTRACT: &[&str] = &["social", "entertainment"];

const DEFAULTS: &[(&str, &str)] = &[
    // desktop apps (WM_CLASS / exe, lowercase)
    ("code", "work"), ("code-oss", "work"), ("vscodium", "work"), ("nvim", "work"), ("vim", "work"), ("emacs", "work"),
    ("jetbrains-idea", "work"), ("jetbrains-pycharm", "work"), ("jetbrains-studio", "work"), ("android studio", "work"),
    ("alacritty", "work"), ("kitty", "work"), ("xterm", "work"), ("urxvt", "work"), ("gnome-terminal", "work"), ("wezterm", "work"),
    ("windowsterminal", "work"), ("devenv", "work"), ("libreoffice", "work"), ("soffice", "work"), ("gimp", "work"), ("inkscape", "work"),
    ("blender", "work"), ("thunderbird", "work"), ("obsidian", "study"), ("anki", "study"), ("zotero", "study"), ("okular", "study"),
    ("zathura", "study"), ("evince", "study"), ("xournalpp", "study"), ("discord", "social"), ("telegram-desktop", "social"),
    ("telegramdesktop", "social"), ("signal", "social"), ("element", "social"), ("slack", "work"), ("zoom", "work"), ("teams", "work"),
    ("spotify", "entertainment"), ("vlc", "entertainment"), ("mpv", "entertainment"), ("steam", "entertainment"),
    ("steamwebhelper", "entertainment"), ("lutris", "entertainment"),
    // android labels and packages
    ("whatsapp", "social"), ("com.whatsapp", "social"), ("instagram", "social"), ("com.instagram.android", "social"),
    ("messenger", "social"), ("com.facebook.orca", "social"), ("facebook", "social"), ("com.facebook.katana", "social"), ("com.facebook.lite", "social"), ("org.telegram.messenger.web", "social"), ("org.thunderdog.challegram", "social"),
    ("telegram", "social"), ("org.telegram.messenger", "social"), ("messages", "social"), ("com.google.android.apps.messaging", "social"),
    ("snapchat", "social"), ("x", "social"), ("com.twitter.android", "social"), ("reddit", "social"), ("com.reddit.frontpage", "social"),
    ("youtube", "entertainment"), ("com.google.android.youtube", "entertainment"), ("netflix", "entertainment"),
    ("com.netflix.mediaclient", "entertainment"), ("tiktok", "entertainment"), ("com.zhiliaoapp.musically", "entertainment"),
    ("com.spotify.music", "entertainment"), ("duolingo", "study"), ("com.duolingo", "study"), ("ankidroid", "study"),
    ("com.ichi2.anki", "study"), ("gmail", "work"), ("com.google.android.gm", "work"), ("calendar", "work"),
    ("com.google.android.calendar", "work"), ("docs", "work"), ("com.google.android.apps.docs.editors.docs", "work"),
    ("drive", "work"), ("agenda", "work"), ("com.selfkit.agenda", "work"), ("hisab", "work"), ("chrono", "other"),
    ("com.google.android.dialer", "social"), ("phone", "social"), ("contacts", "social"), ("camera", "other"), ("maps", "other"),
    ("settings", "other"), ("clock", "other"),
    // websites
    ("github.com", "work"), ("gitlab.com", "work"), ("stackoverflow.com", "work"), ("docs.rs", "work"), ("crates.io", "work"),
    ("developer.mozilla.org", "work"), ("docs.google.com", "work"), ("mail.google.com", "work"), ("notion.so", "work"),
    ("figma.com", "work"), ("chatgpt.com", "work"), ("claude.ai", "work"), ("linear.app", "work"),
    ("wikipedia.org", "study"), ("khanacademy.org", "study"), ("coursera.org", "study"), ("edx.org", "study"),
    ("arxiv.org", "study"), ("scholar.google.com", "study"), ("duolingo.com", "study"), ("brilliant.org", "study"),
    ("leetcode.com", "study"), ("youtube.com", "entertainment"), ("netflix.com", "entertainment"), ("twitch.tv", "entertainment"),
    ("primevideo.com", "entertainment"), ("spotify.com", "entertainment"), ("9gag.com", "entertainment"),
    ("reddit.com", "social"), ("x.com", "social"), ("twitter.com", "social"), ("facebook.com", "social"),
    ("instagram.com", "social"), ("web.whatsapp.com", "social"), ("discord.com", "social"), ("linkedin.com", "social"),
    ("news.ycombinator.com", "social"), ("tiktok.com", "entertainment"),
];

const BROWSERS: &[&str] = &[
    "firefox", "firefox-esr", "librewolf", "zen", "chrome", "google-chrome", "chromium", "chromium-browser", "brave",
    "brave-browser", "msedge", "microsoft-edge", "vivaldi", "vivaldi-stable", "opera", "navigator", "mozilla firefox",
    "com.android.chrome", "org.mozilla.firefox", "com.brave.browser", "com.microsoft.emmx", "chrome.exe",
];

pub fn is_browser(app: &str) -> bool {
    let a = app.to_lowercase();
    let a = a.trim_end_matches(".exe");
    BROWSERS.contains(&a)
}

fn default_for(key: &str) -> Option<&'static str> {
    DEFAULTS.iter().find(|(k, _)| *k == key).map(|(_, c)| *c)
}

/// Category of an app (by label/name, then package) using settings, then defaults.
pub fn app_category(s: &ChronoSettings, app: &str, package: Option<&str>) -> String {
    let lo = app.to_lowercase();
    for k in [Some(app), package].into_iter().flatten() {
        if let Some(c) = s.categories.get(k) {
            return c.clone();
        }
    }
    if let Some((_, c)) = s.categories.iter().find(|(k, _)| k.to_lowercase() == lo) {
        return c.clone();
    }
    if let Some(c) = default_for(&lo).or_else(|| package.and_then(|p| default_for(&p.to_lowercase()))) {
        return c.into();
    }
    if lo.starts_with("jetbrains-") {
        return "work".into();
    }
    "other".into()
}

/// Category of a domain: exact, then parent domains, in settings then defaults.
pub fn domain_category(s: &ChronoSettings, domain: &str) -> String {
    let d = norm_domain(domain);
    let mut cur = d.as_str();
    loop {
        if let Some(c) = s.categories.get(cur) {
            return c.clone();
        }
        if let Some(c) = default_for(cur) {
            return c.into();
        }
        match cur.find('.') {
            Some(i) if cur[i + 1..].contains('.') => cur = &cur[i + 1..],
            _ => break,
        }
    }
    "other".into()
}

/// Static category (no time rules): settings mapping, then defaults, then other.
pub fn static_category(s: &ChronoSettings, kind: char, name: &str, package: Option<&str>) -> String {
    if kind == 'w' { domain_category(s, name) } else { app_category(s, name, package) }
}

/// Does this enabled rule apply to the app (kind 'a') / website (kind 'w')?
pub fn rule_applies(r: &CategoryRule, kind: char, name: &str, package: Option<&str>) -> bool {
    if !r.enabled || r.days.is_empty() {
        return false;
    }
    match (r.kind.as_str(), kind) {
        ("domain", 'w') => domain_matches(name, &r.target),
        ("app", 'a') => r.target.eq_ignore_ascii_case(name) || package.is_some_and(|p| r.target.eq_ignore_ascii_case(p)),
        _ => false,
    }
}

/// Enabled rules for one app/site, highest priority first: the most specific
/// domain (`music.youtube.com` before `youtube.com`), then list order.
pub fn rules_for<'a>(s: &'a ChronoSettings, kind: char, name: &str, package: Option<&str>) -> Vec<&'a CategoryRule> {
    let mut v: Vec<(usize, &CategoryRule)> =
        s.category_rules.iter().enumerate().filter(|(_, r)| rule_applies(r, kind, name, package)).collect();
    v.sort_by_key(|(i, r)| (std::cmp::Reverse(if r.kind == "domain" { r.target.len() } else { 0 }), *i));
    v.into_iter().map(|(_, r)| r).collect()
}

/// Window length in minutes: end after start = same day; end ≤ start runs
/// into the next day; equal = the whole 24 h.
fn window_min(r: &CategoryRule) -> i64 {
    let (a, b) = (r.start_min.clamp(0, 1439), r.end_min.clamp(0, 1440) % 1440);
    if b > a { b - a } else { 1440 - a + b }
}

/// Parts of [a, b) (UTC ms; `tz` = minutes east of UTC, the session's own
/// offset) that fall inside a rule window, as disjoint sorted
/// (start, end, category). `rules` in priority order (see `rules_for`);
/// where windows overlap the first rule wins. Days are calendar days of the
/// window start in local clock time (1 = Monday), independent of the day-start
/// setting; an overnight window belongs to the day it starts on.
pub fn rule_pieces(rules: &[&CategoryRule], a: i64, b: i64, tz: i32) -> Vec<(i64, i64, String)> {
    if rules.is_empty() || b <= a {
        return vec![];
    }
    let off = tz as i64 * MIN;
    let (d0, d1) = ((a + off).div_euclid(DAY) - 1, (b + off).div_euclid(DAY));
    let mut wins: Vec<(i64, i64, usize)> = Vec::new();
    for (pri, r) in rules.iter().enumerate() {
        let len = window_min(r) * MIN;
        for d in d0..=d1 {
            let wd = ((d + 3).rem_euclid(7) + 1) as u8; // 1970-01-01 was a Thursday
            if !r.days.contains(&wd) {
                continue;
            }
            let ws = d * DAY - off + r.start_min.clamp(0, 1439) * MIN;
            let (s, e) = (ws.max(a), (ws + len).min(b));
            if e > s {
                wins.push((s, e, pri));
            }
        }
    }
    if wins.is_empty() {
        return vec![];
    }
    let mut cuts: Vec<i64> = wins.iter().flat_map(|w| [w.0, w.1]).collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<(i64, i64, String)> = Vec::new();
    for c in cuts.windows(2) {
        let (s, e) = (c[0], c[1]);
        let Some(p) = wins.iter().filter(|w| w.0 <= s && w.1 >= e).map(|w| w.2).min() else { continue };
        let cat = &rules[p].category;
        match out.last_mut() {
            Some(l) if l.1 == s && &l.2 == cat => l.1 = e,
            _ => out.push((s, e, cat.clone())),
        }
    }
    out
}

/// [a, b) split into (start, end, category) covering it completely: rule
/// pieces, and the static category in between.
pub fn category_pieces(s: &ChronoSettings, kind: char, name: &str, package: Option<&str>, a: i64, b: i64, tz: i32) -> Vec<(i64, i64, String)> {
    let base = static_category(s, kind, name, package);
    let rules = rules_for(s, kind, name, package);
    let mut out = Vec::new();
    let mut t = a;
    for (x, y, c) in rule_pieces(&rules, a, b, tz) {
        if x > t {
            out.push((t, x, base.clone()));
        }
        out.push((x, y, c));
        t = y;
    }
    if b > t {
        out.push((t, b, base));
    }
    out
}

/// The category of an app/site at instant `t` (UTC ms, local offset `tz`):
/// an enabled matching rule > static mapping > defaults > other.
pub fn category_at(s: &ChronoSettings, kind: char, name: &str, package: Option<&str>, t: i64, tz: i32) -> String {
    let rules = rules_for(s, kind, name, package);
    rule_pieces(&rules, t, t + 1, tz).pop().map(|p| p.2).unwrap_or_else(|| static_category(s, kind, name, package))
}

/// Focus score 0–100 from category totals; None when nothing is categorised.
pub fn focus_score(cats: &std::collections::BTreeMap<String, i64>) -> Option<i64> {
    let f: i64 = FOCUS.iter().map(|c| cats.get(*c).copied().unwrap_or(0)).sum();
    let d: i64 = DISTRACT.iter().map(|c| cats.get(*c).copied().unwrap_or(0)).sum();
    (f + d > 0).then(|| ((f as f64) * 100.0 / ((f + d) as f64)).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping() {
        let mut s = ChronoSettings::default();
        assert_eq!(app_category(&s, "code", None), "work");
        assert_eq!(app_category(&s, "WhatsApp", Some("com.whatsapp")), "social");
        assert_eq!(app_category(&s, "Unknown", Some("com.x.y")), "other");
        assert_eq!(app_category(&s, "Lite", Some("com.facebook.lite")), "social"); // P24
        assert_eq!(domain_category(&s, "www.youtube.com"), "entertainment");
        assert_eq!(domain_category(&s, "en.wikipedia.org"), "study");
        s.categories.insert("youtube.com".into(), "study".into());
        assert_eq!(domain_category(&s, "m.youtube.com"), "study");
        s.categories.insert("Code".into(), "study".into());
        assert_eq!(app_category(&s, "code", None), "study");
        assert!(is_browser("Firefox") && is_browser("chrome.exe") && !is_browser("code"));
        let mut c = std::collections::BTreeMap::new();
        c.insert("work".to_string(), 30);
        c.insert("social".to_string(), 10);
        c.insert("other".to_string(), 100);
        assert_eq!(focus_score(&c), Some(75));
    }

    // ---- time rules -------------------------------------------------------

    const H: i64 = 3_600_000;
    /// 2026-11-16 is a Monday (day 20773).
    const MON: i64 = 20773;

    fn at(day: i64, h: i64, m: i64, tz: i32) -> i64 {
        day * DAY + h * H + m * MIN - tz as i64 * MIN
    }

    fn rule(id: &str, kind: &str, target: &str, cat: &str, days: &[u8], from: (i64, i64), to: (i64, i64)) -> CategoryRule {
        CategoryRule {
            id: id.into(),
            target: target.into(),
            kind: kind.into(),
            category: cat.into(),
            days: days.to_vec(),
            start_min: from.0 * 60 + from.1,
            end_min: to.0 * 60 + to.1,
            enabled: true,
        }
    }

    #[test]
    fn rule_matching_weekdays_and_subdomains() {
        assert_eq!(crate::stats::weekday(MON), 0);
        let mut s = ChronoSettings::default();
        s.category_rules.push(rule("r1", "domain", "youtube.com", "work", &[1, 2, 3, 4, 5], (9, 0), (17, 0)));
        let c = |name: &str, t: i64, tz: i32| category_at(&s, 'w', name, None, t, tz);
        assert_eq!(c("youtube.com", at(MON, 10, 0, 0), 0), "work");
        assert_eq!(c("m.youtube.com", at(MON, 9, 0, 0), 0), "work"); // subdomain, start inclusive
        assert_eq!(c("youtube.com", at(MON, 17, 0, 0), 0), "entertainment"); // end exclusive
        assert_eq!(c("youtube.com", at(MON, 8, 59, 0), 0), "entertainment");
        assert_eq!(c("youtube.com", at(MON + 5, 10, 0, 0), 0), "entertainment"); // Saturday
        assert_eq!(c("notyoutube.com", at(MON, 10, 0, 0), 0), "other");
        // local clock of the session: 10:00 in UTC+5:30 is 04:30 UTC
        assert_eq!(c("youtube.com", at(MON, 10, 0, 330), 330), "work");
        assert_eq!(c("youtube.com", at(MON, 10, 0, 330), 0), "entertainment");
        // app rules match the name or the package, any case; not websites
        s.category_rules.push(rule("r2", "app", "com.whatsapp", "work", &[1], (0, 0), (0, 0)));
        assert_eq!(category_at(&s, 'a', "WhatsApp", Some("com.whatsapp"), at(MON, 13, 0, 0), 0), "work");
        assert_eq!(category_at(&s, 'a', "WhatsApp", Some("COM.WHATSAPP"), at(MON, 23, 59, 0), 0), "work"); // equal start/end = 24 h
        assert_eq!(category_at(&s, 'a', "WhatsApp", Some("com.whatsapp"), at(MON + 1, 0, 0, 0), 0), "social");
        // an app rule never applies to a website of the same name (static category stays)
        assert_eq!(category_at(&s, 'w', "com.whatsapp", None, at(MON, 13, 0, 0), 0), static_category(&s, 'w', "com.whatsapp", None));
    }

    #[test]
    fn overnight_rules_belong_to_their_start_day() {
        let mut s = ChronoSettings::default();
        // Friday 22:00 → Saturday 02:00
        s.category_rules.push(rule("r1", "app", "Telegram", "work", &[5], (22, 0), (2, 0)));
        let c = |d: i64, h: i64| category_at(&s, 'a', "Telegram", None, at(d, h, 0, 0), 0);
        assert_eq!(c(MON + 4, 23), "work"); // Fri 23:00
        assert_eq!(c(MON + 5, 1), "work"); // Sat 01:00
        assert_eq!(c(MON + 5, 2), "social"); // Sat 02:00 (end)
        assert_eq!(c(MON + 3, 1), "social"); // Thu 01:00: Wednesday night isn't in the rule
        assert_eq!(c(MON + 5, 23), "social"); // Sat 23:00
        // one session Fri 21:00 → Sat 03:00 splits in three
        let rules = rules_for(&s, 'a', "Telegram", None);
        let p = category_pieces(&s, 'a', "Telegram", None, at(MON + 4, 21, 0, 0), at(MON + 5, 3, 0, 0), 0);
        assert_eq!(rules.len(), 1);
        assert_eq!(
            p,
            vec![
                (at(MON + 4, 21, 0, 0), at(MON + 4, 22, 0, 0), "social".to_string()),
                (at(MON + 4, 22, 0, 0), at(MON + 5, 2, 0, 0), "work".to_string()),
                (at(MON + 5, 2, 0, 0), at(MON + 5, 3, 0, 0), "social".to_string()),
            ]
        );
        // Sunday night into Monday wraps the week (day 7 → Monday)
        s.category_rules[0].days = vec![7];
        let c = |d: i64, h: i64| category_at(&s, 'a', "Telegram", None, at(d, h, 0, 0), 0);
        assert_eq!(c(MON, 1), "work");
        assert_eq!(c(MON + 7, 1), "work");
        assert_eq!(c(MON + 6, 1), "social"); // Saturday night isn't in it
    }

    #[test]
    fn dst_uses_each_sessions_own_offset() {
        // Same 09:00–17:00 rule, before (UTC+1) and after (UTC+2) a DST change.
        let mut s = ChronoSettings::default();
        s.category_rules.push(rule("r1", "domain", "reddit.com", "study", &[1, 2, 3, 4, 5, 6, 7], (9, 0), (17, 0)));
        let rules = rules_for(&s, 'w', "reddit.com", None);
        for tz in [60, 120] {
            // 08:00–10:00 local → only 09:00–10:00 local is in the rule
            let p = rule_pieces(&rules, at(MON, 8, 0, tz), at(MON, 10, 0, tz), tz);
            assert_eq!(p, vec![(at(MON, 9, 0, tz), at(MON, 10, 0, tz), "study".to_string())]);
        }
        // the same UTC instant is 08:30 in winter (outside) and 09:30 in summer (inside)
        let t = at(MON, 7, 30, 0);
        assert_eq!(category_at(&s, 'w', "reddit.com", None, t, 60), "social");
        assert_eq!(category_at(&s, 'w', "reddit.com", None, t, 120), "study");
    }

    #[test]
    fn dst_change_days_with_a_night_rule() {
        // Rule every day 01:00–03:00 local. Europe/Berlin-style changes in 2026.
        let mut s = ChronoSettings::default();
        s.category_rules.push(rule("r1", "domain", "reddit.com", "study", &[1, 2, 3, 4, 5, 6, 7], (1, 0), (3, 0)));
        let rules = rules_for(&s, 'w', "reddit.com", None);
        // Spring forward, Sun 29 Mar (day 20541): 02:00 CET → 03:00 CEST at 01:00 UTC.
        // A session 00:30 CET → 04:00 CEST keeps its start offset (+60), like the
        // day split: the rule covers 01:00–03:00 in that clock = 00:00–02:00 UTC.
        let spring = 20541;
        let (a, b) = (at(spring, 0, 30, 60), at(spring, 4, 0, 120));
        assert_eq!(b, at(spring, 3, 0, 60));
        assert_eq!(rule_pieces(&rules, a, b, 60), vec![(at(spring, 1, 0, 60), b, "study".to_string())]);
        // A session that starts after the change (03:30 CEST) is outside the rule.
        assert!(rule_pieces(&rules, at(spring, 3, 30, 120), at(spring, 5, 0, 120), 120).is_empty());
        // Fall back, Sun 25 Oct (day 20751): 03:00 CEST → 02:00 CET at 01:00 UTC; the hour
        // 02:00–03:00 happens twice and both are inside the rule (each in its own offset).
        let fall = 20751;
        let first = rule_pieces(&rules, at(fall, 2, 30, 120), at(fall, 3, 30, 120), 120);
        assert_eq!(first, vec![(at(fall, 2, 30, 120), at(fall, 3, 0, 120), "study".to_string())]);
        let second = rule_pieces(&rules, at(fall, 2, 30, 60), at(fall, 3, 30, 60), 60);
        assert_eq!(second, vec![(at(fall, 2, 30, 60), at(fall, 3, 0, 60), "study".to_string())]);
        assert_eq!(at(fall, 3, 0, 120), at(fall, 2, 0, 60)); // the change itself
        assert!(first[0].1 <= second[0].0);
    }

    #[test]
    fn one_session_across_two_rule_windows() {
        let mut s = ChronoSettings::default();
        s.category_rules.push(rule("r1", "domain", "youtube.com", "work", &[1], (9, 0), (12, 0)));
        s.category_rules.push(rule("r2", "domain", "youtube.com", "study", &[1], (14, 0), (17, 0)));
        let p = category_pieces(&s, 'w', "youtube.com", None, at(MON, 11, 0, 0), at(MON, 15, 0, 0), 0);
        assert_eq!(
            p,
            vec![
                (at(MON, 11, 0, 0), at(MON, 12, 0, 0), "work".to_string()),
                (at(MON, 12, 0, 0), at(MON, 14, 0, 0), "entertainment".to_string()),
                (at(MON, 14, 0, 0), at(MON, 15, 0, 0), "study".to_string()),
            ]
        );
        // The same rule on two days: a session Mon 16:00 → Tue 10:00 hits both windows.
        s.category_rules[0].days = vec![1, 2];
        s.category_rules[0].end_min = 17 * 60;
        s.category_rules.remove(1);
        let p = category_pieces(&s, 'w', "youtube.com", None, at(MON, 16, 0, 0), at(MON + 1, 10, 0, 0), 0);
        let work: i64 = p.iter().filter(|x| x.2 == "work").map(|x| x.1 - x.0).sum();
        assert_eq!(work, 2 * H);
        assert_eq!(p.len(), 3);
    }

    #[test]
    fn resolution_order_and_priority() {
        let mut s = ChronoSettings::default();
        let t = at(MON, 10, 0, 0);
        assert_eq!(category_at(&s, 'w', "youtube.com", None, t, 0), "entertainment"); // default
        s.categories.insert("youtube.com".into(), "study".into());
        assert_eq!(category_at(&s, 'w', "youtube.com", None, t, 0), "study"); // static beats default
        s.category_rules.push(rule("r1", "domain", "youtube.com", "work", &[1], (9, 0), (12, 0)));
        assert_eq!(category_at(&s, 'w', "youtube.com", None, t, 0), "work"); // rule beats static
        assert_eq!(category_at(&s, 'w', "youtube.com", None, at(MON, 13, 0, 0), 0), "study");
        s.category_rules[0].enabled = false;
        assert_eq!(category_at(&s, 'w', "youtube.com", None, t, 0), "study"); // disabled rule ignored
        s.category_rules[0].enabled = true;
        s.category_rules[0].days.clear();
        assert_eq!(category_at(&s, 'w', "youtube.com", None, t, 0), "study"); // no days = never
        s.category_rules[0].days = vec![1];
        // more specific domain wins, then list order
        s.category_rules.push(rule("r2", "domain", "youtube.com", "social", &[1], (9, 0), (12, 0)));
        s.category_rules.push(rule("r3", "domain", "music.youtube.com", "entertainment", &[1], (10, 0), (11, 0)));
        assert_eq!(category_at(&s, 'w', "youtube.com", None, t, 0), "work");
        assert_eq!(category_at(&s, 'w', "music.youtube.com", None, t, 0), "entertainment");
        let rules = rules_for(&s, 'w', "music.youtube.com", None);
        let p = rule_pieces(&rules, at(MON, 9, 30, 0), at(MON, 11, 30, 0), 0);
        assert_eq!(
            p,
            vec![
                (at(MON, 9, 30, 0), at(MON, 10, 0, 0), "work".to_string()),
                (at(MON, 10, 0, 0), at(MON, 11, 0, 0), "entertainment".to_string()),
                (at(MON, 11, 0, 0), at(MON, 11, 30, 0), "work".to_string()),
            ]
        );
        // no mapping anywhere → other
        assert_eq!(category_at(&s, 'a', "some-tool", None, t, 0), "other");
    }
}
