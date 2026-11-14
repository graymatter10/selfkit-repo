//! Platform activity probes. Each returns the foreground app name (no window
//! titles — privacy default) and seconds since last user input.

#[cfg(target_os = "linux")]
mod x11;
#[cfg(windows)]
mod win;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// Lowercase app identifier, e.g. "firefox", "code". None = desktop/no window.
    pub app: Option<String>,
    pub idle_secs: u64,
}

pub trait Probe: Send {
    fn sample(&mut self) -> anyhow::Result<Sample>;
}

pub fn platform_probe() -> anyhow::Result<Box<dyn Probe>> {
    #[cfg(target_os = "linux")]
    return Ok(Box::new(x11::X11Probe::connect()?));
    #[cfg(windows)]
    return Ok(Box::new(win::WinProbe));
    #[allow(unreachable_code)]
    Err(anyhow::anyhow!("no activity probe for this platform"))
}
