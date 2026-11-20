//! Installers: native-messaging host manifests for Chrome, Chromium and Firefox
//! (Linux files; Windows registry keys), and the autostart entry (Linux:
//! systemd user unit; Windows: per-user logon task).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::native_host::HOST_NAME;

/// Chrome extension id, fixed by the public `key` in extension/manifest.json.
pub const CHROME_EXTENSION_ID: &str = "ijiimbgfojojhjlchgbkkhhodifbnami";
/// Firefox add-on id (`browser_specific_settings.gecko.id`).
pub const FIREFOX_EXTENSION_ID: &str = "chrono@selfkit";

pub fn chrome_manifest(exe: &Path, extra_ids: &[String]) -> Value {
    let mut origins = vec![format!("chrome-extension://{CHROME_EXTENSION_ID}/")];
    origins.extend(extra_ids.iter().map(|id| format!("chrome-extension://{id}/")));
    json!({
        "name": HOST_NAME,
        "description": "Selfkit agent: records browser time for Chrono",
        "path": exe.display().to_string(),
        "type": "stdio",
        "allowed_origins": origins,
    })
}

pub fn firefox_manifest(exe: &Path) -> Value {
    json!({
        "name": HOST_NAME,
        "description": "Selfkit agent: records browser time for Chrono",
        "path": exe.display().to_string(),
        "type": "stdio",
        "allowed_extensions": [FIREFOX_EXTENSION_ID],
    })
}

fn home() -> Result<PathBuf> {
    dirs_home().context("no home directory")
}

fn dirs_home() -> Option<PathBuf> {
    #[cfg(windows)]
    return std::env::var_os("USERPROFILE").map(PathBuf::from);
    #[allow(unreachable_code)]
    std::env::var_os("HOME").map(PathBuf::from)
}

fn write_json(p: &Path, v: &Value) -> Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, serde_json::to_vec_pretty(v)?)?;
    Ok(())
}

pub struct NativeHostOpts {
    pub exe: PathBuf,
    /// Extra Chrome extension ids (e.g. an unpacked copy without the key).
    pub extra_ids: Vec<String>,
    /// Extra Chromium-style user-data dirs (`<dir>/NativeMessagingHosts`).
    pub profile_dirs: Vec<PathBuf>,
    pub uninstall: bool,
}

/// Linux: per-user manifest locations.
#[cfg(not(windows))]
pub fn native_host_targets(home: &Path) -> Vec<(&'static str, PathBuf)> {
    let file = format!("{HOST_NAME}.json");
    vec![
        ("chrome", home.join(".config/google-chrome/NativeMessagingHosts").join(&file)),
        ("chromium", home.join(".config/chromium/NativeMessagingHosts").join(&file)),
        ("firefox", home.join(".mozilla/native-messaging-hosts").join(&file)),
    ]
}

/// Windows: manifest folder, Chrome-style and Firefox manifests, and the HKCU keys
/// (full key path, manifest it points at) for Chrome, Chromium, Edge and Firefox.
#[cfg(any(windows, test))]
#[allow(clippy::type_complexity)]
pub fn windows_native_host_targets(local_app_data: &Path) -> (PathBuf, PathBuf, PathBuf, Vec<(String, PathBuf)>) {
    let dir = local_app_data.join("Selfkit").join("native-host");
    let cj = dir.join(format!("{HOST_NAME}.chrome.json"));
    let fj = dir.join(format!("{HOST_NAME}.firefox.json"));
    let keys = [
        (r"HKCU\Software\Google\Chrome\NativeMessagingHosts", &cj),
        (r"HKCU\Software\Chromium\NativeMessagingHosts", &cj),
        (r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts", &cj),
        (r"HKCU\Software\Mozilla\NativeMessagingHosts", &fj),
    ]
    .into_iter()
    .map(|(k, f)| (format!(r"{k}\{HOST_NAME}"), f.clone()))
    .collect();
    (dir, cj, fj, keys)
}

/// Write (or remove) the host manifests; returns what was done.
pub fn install_native_host(o: &NativeHostOpts) -> Result<Vec<String>> {
    let mut done = vec![];
    let chrome = chrome_manifest(&o.exe, &o.extra_ids);
    let firefox = firefox_manifest(&o.exe);
    #[cfg(not(windows))]
    {
        let mut targets = native_host_targets(&home()?);
        for d in &o.profile_dirs {
            targets.push(("chromium", d.join("NativeMessagingHosts").join(format!("{HOST_NAME}.json"))));
        }
        for (browser, p) in targets {
            if o.uninstall {
                if std::fs::remove_file(&p).is_ok() {
                    done.push(format!("removed {}", p.display()));
                }
                continue;
            }
            write_json(&p, if browser == "firefox" { &firefox } else { &chrome })?;
            done.push(format!("{browser}: {}", p.display()));
        }
    }
    #[cfg(windows)]
    {
        // Manifests in %LOCALAPPDATA%\Selfkit\native-host, registered under HKCU.
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).context("LOCALAPPDATA not set")?;
        let (dir, cj, fj, keys) = windows_native_host_targets(&local);
        if !o.uninstall {
            write_json(&cj, &chrome)?;
            write_json(&fj, &firefox)?;
        }
        for (key, file) in keys {
            let st = if o.uninstall {
                std::process::Command::new("reg").args(["delete", &key, "/f"]).status()?
            } else {
                std::process::Command::new("reg").args(["add", &key, "/ve", "/t", "REG_SZ", "/d", &file.display().to_string(), "/f"]).status()?
            };
            if st.success() {
                done.push(format!("{} {key}", if o.uninstall { "removed" } else { "registered" }));
            }
        }
        if o.uninstall {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    Ok(done)
}

pub fn systemd_unit(exe: &Path) -> String {
    format!(
        "# Selfkit agent — written by `selfkit-agent install-service`. Docs: docs/platforms/linux.md
[Unit]
Description=Selfkit agent (activity tracking, sync, backups, reminders)
After=graphical-session.target

[Service]
ExecStart={} run
Restart=on-failure
RestartSec=10
MemoryMax=64M
Nice=5

[Install]
WantedBy=default.target
",
        exe.display()
    )
}

pub const I3_SNIPPET: &str = "# ~/.config/i3/config — give the user service the X display, then (re)start it
exec --no-startup-id systemctl --user import-environment DISPLAY XAUTHORITY && systemctl --user restart selfkit-agent
# …or without systemd:
# exec --no-startup-id ~/.local/bin/selfkit-agent run";

/// Linux: write the systemd user unit (and enable it when asked).
/// Windows: create/delete the per-user logon task.
pub fn install_service(exe: &Path, enable: bool, uninstall: bool) -> Result<Vec<String>> {
    let mut done = vec![];
    #[cfg(not(windows))]
    {
        let p = home()?.join(".config/systemd/user/selfkit-agent.service");
        let systemctl = |args: &[&str]| std::process::Command::new("systemctl").arg("--user").args(args).status();
        if uninstall {
            if enable {
                let _ = systemctl(&["disable", "--now", "selfkit-agent"]);
            }
            if std::fs::remove_file(&p).is_ok() {
                done.push(format!("removed {}", p.display()));
            }
            return Ok(done);
        }
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(&p, systemd_unit(exe))?;
        done.push(format!("wrote {}", p.display()));
        if enable {
            systemctl(&["daemon-reload"])?;
            let st = systemctl(&["enable", "--now", "selfkit-agent"])?;
            anyhow::ensure!(st.success(), "systemctl enable failed");
            done.push("enabled and started selfkit-agent.service".into());
        } else {
            done.push("next: systemctl --user daemon-reload && systemctl --user enable --now selfkit-agent".into());
        }
        done.push(I3_SNIPPET.into());
    }
    #[cfg(windows)]
    {
        let _ = enable;
        // Per-user Run key instead of a logon task: `schtasks /SC ONLOGON` needs an elevated
        // prompt, so a standard user got no autostart (C28). Old logon tasks are removed.
        const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let _ = std::process::Command::new("schtasks").args(["/Delete", "/TN", "Selfkit agent", "/F"]).status();
        let st = if uninstall {
            std::process::Command::new("reg").args(["delete", RUN, "/v", "Selfkit agent", "/f"]).status()?
        } else {
            // Same value as the installer (agent.nsi RUNVALUE), and its hidden launcher when present.
            let launcher = exe.with_file_name("selfkit-agent-start.exe");
            let cmd = if launcher.exists() { format!("\"{}\"", launcher.display()) } else { format!("\"{}\" run", exe.display()) };
            std::process::Command::new("reg").args(["add", RUN, "/v", "Selfkit agent", "/t", "REG_SZ", "/d", &cmd, "/f"]).status()?
        };
        anyhow::ensure!(st.success() || uninstall, "reg failed");
        done.push(if uninstall { "removed the autostart entry (HKCU Run \"Selfkit agent\")".into() } else { "added autostart (HKCU Run \"Selfkit agent\")".into() });
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests() {
        let c = chrome_manifest(Path::new("/opt/selfkit-agent"), &["abc".into()]);
        assert_eq!(c["name"], "com.selfkit.agent");
        assert_eq!(c["allowed_origins"][0], format!("chrome-extension://{CHROME_EXTENSION_ID}/"));
        assert_eq!(c["allowed_origins"][1], "chrome-extension://abc/");
        let f = firefox_manifest(Path::new("/opt/selfkit-agent"));
        assert_eq!(f["allowed_extensions"][0], "chrono@selfkit");
        assert_eq!(f["path"], "/opt/selfkit-agent");
        assert!(systemd_unit(Path::new("/x/agent")).contains("ExecStart=/x/agent run"));
    }

    #[test]
    fn windows_targets() {
        let (dir, cj, fj, keys) = windows_native_host_targets(Path::new("C:/Users/u/AppData/Local"));
        assert_eq!(dir, Path::new("C:/Users/u/AppData/Local").join("Selfkit").join("native-host"));
        assert_eq!(cj, dir.join("com.selfkit.agent.chrome.json"));
        assert_eq!(fj, dir.join("com.selfkit.agent.firefox.json"));
        assert_eq!(keys.len(), 4);
        assert_eq!(keys[2].0, r"HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.selfkit.agent");
        assert_eq!(keys[2].1, cj);
        assert_eq!(keys[3].0, r"HKCU\Software\Mozilla\NativeMessagingHosts\com.selfkit.agent");
        assert_eq!(keys[3].1, fj);
    }

    /// Real round trip on Windows: manifests + HKCU keys written, then removed. Uses a
    /// temporary LOCALAPPDATA; the HKCU keys are the real per-user ones, so it only runs
    /// with --ignored (CI on windows-latest does).
    #[cfg(windows)]
    #[test]
    #[ignore = "writes HKCU registry keys; run on CI with --ignored"]
    fn windows_native_host_round_trip() {
        let tmp = std::env::temp_dir().join(format!("selfkit-nh-{}", std::process::id()));
        // SAFETY: test-only; no other thread in this test reads LOCALAPPDATA.
        unsafe { std::env::set_var("LOCALAPPDATA", &tmp) };
        let exe = PathBuf::from(r"C:\Program Files\x\selfkit-agent.exe");
        let mut o = NativeHostOpts { exe: exe.clone(), extra_ids: vec![], profile_dirs: vec![], uninstall: false };
        let done = install_native_host(&o).unwrap();
        assert_eq!(done.iter().filter(|d| d.starts_with("registered")).count(), 4, "{done:?}");
        let (_, cj, _, keys) = windows_native_host_targets(&tmp);
        let m: Value = serde_json::from_slice(&std::fs::read(&cj).unwrap()).unwrap();
        assert_eq!(m["path"], exe.display().to_string());
        for (k, f) in &keys {
            let out = std::process::Command::new("reg").args(["query", k, "/ve"]).output().unwrap();
            assert!(out.status.success(), "{k} missing");
            assert!(String::from_utf8_lossy(&out.stdout).contains(&f.display().to_string()), "{k} value");
        }
        o.uninstall = true;
        install_native_host(&o).unwrap();
        for (k, _) in &keys {
            assert!(!std::process::Command::new("reg").args(["query", k]).output().unwrap().status.success(), "{k} still there");
        }
        assert!(!cj.exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(not(windows))]
    #[test]
    fn linux_targets() {
        let t = native_host_targets(Path::new("/home/u"));
        assert_eq!(t[2].1, Path::new("/home/u/.mozilla/native-messaging-hosts/com.selfkit.agent.json"));
        assert_eq!(t[1].1, Path::new("/home/u/.config/chromium/NativeMessagingHosts/com.selfkit.agent.json"));
    }
}
