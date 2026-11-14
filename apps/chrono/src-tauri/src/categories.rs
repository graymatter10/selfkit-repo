//! Built-in category defaults (overridden by `chrono_settings.categories`)
//! and browser detection. Categories: study, work, social, entertainment, other.

use crate::settings::{ChronoSettings, norm_domain};

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
    ("messenger", "social"), ("com.facebook.orca", "social"), ("facebook", "social"), ("com.facebook.katana", "social"),
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
}
