//! Desktop: talk to the running selfkit-agent (owner = Agent). JSON lines over
//! `$XDG_RUNTIME_DIR/selfkit-agent.sock` (Linux) or `\\.\pipe\selfkit-agent` (Windows).

use serde_json::Value;
use selfkit_core::{Error, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[cfg(unix)]
pub fn socket_path() -> std::path::PathBuf {
    std::env::var("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir()).join("selfkit-agent.sock")
}

pub async fn request(req: &Value) -> Result<Value> {
    let fut = async {
        #[cfg(unix)]
        let stream = tokio::net::UnixStream::connect(socket_path()).await?;
        #[cfg(windows)]
        let stream = tokio::net::windows::named_pipe::ClientOptions::new().open(r"\\.\pipe\selfkit-agent")?;
        #[cfg(not(any(unix, windows)))]
        return Err::<Value, Error>(Error::Remote("no agent on this platform".into()));
        #[cfg(any(unix, windows))]
        {
            let (r, mut w) = tokio::io::split(stream);
            w.write_all(format!("{req}\n").as_bytes()).await?;
            let mut line = String::new();
            BufReader::new(r).read_line(&mut line).await?;
            Ok(serde_json::from_str(&line)?)
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), fut).await.map_err(|_| Error::Remote("agent did not answer".into()))?
}

pub async fn running() -> bool {
    request(&serde_json::json!({"cmd": "status"})).await.is_ok()
}

pub async fn sync_now() -> Result<()> {
    request(&serde_json::json!({"cmd": "sync_now"})).await.map(|_| ())
}

/// `status` payload: `{status: {last_sync_at, last_sync_ok_at, last_sync_error, ...}, lan: {peers: [...]}}`.
pub async fn status() -> Result<Value> {
    request(&serde_json::json!({"cmd": "status"})).await
}
