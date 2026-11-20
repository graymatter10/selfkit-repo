//! Turn technical errors into one plain sentence for the status line.

use selfkit_core::Error;

/// Errors this crate raises itself (wrapped in `Error::Remote` / `Error::Format`
/// with these prefixes so they survive the core error type).
pub(crate) const WRONG_KEY: &str = "sk:wrong-key";
pub(crate) const SCHEMA_NEWER: &str = "sk:schema-newer";
pub(crate) const LOCKED: &str = "sk:locked";
pub(crate) const NOTHING: &str = "sk:nothing-configured";
pub(crate) const NO_PEER: &str = "sk:no-peer";
pub(crate) const NO_SERVER: &str = "sk:no-server";
pub(crate) const TIMEOUT: &str = "sk:timeout";

pub fn explain(e: &Error) -> String {
    let t = e.to_string();
    let l = t.to_lowercase();
    let has = |s: &str| l.contains(s);
    if has(WRONG_KEY) || matches!(e, Error::Decrypt | Error::BadKey) {
        return "This device's encryption key doesn't match your other devices. Unlock with your passphrase or recovery key.".into();
    }
    if has(SCHEMA_NEWER) {
        return "The server was set up by a newer Selfkit version. Update this app, then sync again.".into();
    }
    if has(LOCKED) {
        return "Sync is locked. Set up encryption or unlock with your passphrase.".into();
    }
    if has(NOTHING) {
        return "Nothing to sync with yet: no PC found on this Wi-Fi and no server set up.".into();
    }
    if has(NO_PEER) {
        return "No Selfkit PC found on this network. Is the PC on and on the same Wi-Fi?".into();
    }
    if has(NO_SERVER) {
        return "No server set up. Add one in Sync settings, or sync on the same Wi-Fi as your PC.".into();
    }
    if has("peer speaks protocol") {
        return "Your other device runs a different Selfkit version. Update both, then sync again.".into();
    }
    if has("password authentication failed") || has("authentication failed") {
        return "The server refused the user name or password.".into();
    }
    if has("does not exist") && has("database") {
        return "The server is reachable but that database doesn't exist.".into();
    }
    if has("http 403") || has("http 401") || has("accessdenied") || has("signaturedoesnotmatch") || has("invalidaccesskeyid") {
        return "The photo storage refused the access key. Check the key and that it may write to the bucket.".into();
    }
    if has("nosuchbucket") || has("http 404") {
        return "The photo storage bucket wasn't found.".into();
    }
    if has(TIMEOUT) || has("timed out") || has("timeout") {
        return "The connection timed out. Check the internet connection and try again.".into();
    }
    if has("connection refused")
        || has("failed to lookup")
        || has("dns")
        || has("network is unreachable")
        || has("no route to host")
        || has("error connecting")
        || has("connection reset")
    {
        return "Can't reach the server. Check the internet connection or the server address.".into();
    }
    if has("certificate") || has("tls") {
        return "The server's secure connection failed (certificate problem).".into();
    }
    if has("database is locked") || has("busy") {
        return "Another sync is using the data right now. Try again in a moment.".into();
    }
    if has("keychain") {
        // Linux desktops without a Secret Service (common on i3) can't store passwords (L4).
        return if cfg!(all(target_os = "linux", not(target_os = "android"))) {
            "Couldn't use this device's secure storage. Start a keyring (GNOME Keyring, or KeePassXC with Secret Service on) and try again."
        } else {
            "Couldn't read this device's secure storage."
        }
        .into();
    }
    if matches!(e, Error::Io(_)) {
        return "Couldn't read or write files on this device.".into();
    }
    "Sync failed. Details are in the sync log.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_words() {
        assert!(explain(&Error::Remote("error connecting to server: Connection refused (os error 111)".into())).contains("Can't reach"));
        assert!(explain(&Error::Remote(format!("{SCHEMA_NEWER}: 7 > 1"))).contains("newer Selfkit"));
        assert!(explain(&Error::Format("lan: peer speaks protocol 9, we speak 1".into())).contains("different Selfkit version"));
        assert!(explain(&Error::BadKey).contains("encryption key"));
    }
}
