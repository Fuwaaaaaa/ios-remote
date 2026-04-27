//! Auto-spawn `iproxy` for WebDriverAgent so users don't have to manually
//! forward USB port 8100 before macros work. The probe-then-spawn dance is
//! deliberately lenient: any failure mode (already running, not installed,
//! no UDID) just logs and continues — never panics, never blocks startup.
//!
//! On Windows the spawned child does not auto-die when the parent exits.
//! That's fine for v0.6 (iproxy is a tiny user-mode process and dies on
//! USB disconnect anyway), but a follow-up could move this into a Job
//! Object so Windows reaps the child on parent exit.

use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tracing::{info, warn};

/// Probe + spawn `iproxy 8100 8100` if the WDA forwarding port is not yet
/// bound. Returns `Some(child)` on success so the caller can keep the
/// handle alive for the session. Returns `None` if iproxy is already
/// running, not installed, or spawning fails — in all cases the caller
/// should continue normally; macros either work already or won't work in
/// this session.
pub fn try_spawn(udid: Option<&str>) -> Option<Child> {
    if probe_port_8100() {
        info!("iproxy: WDA port 8100 already forwarded — skipping spawn");
        return None;
    }
    let mut cmd = Command::new("iproxy");
    cmd.args(["8100", "8100"]);
    if let Some(u) = udid {
        cmd.args(["-u", u]);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    match cmd.spawn() {
        Ok(child) => {
            info!(
                pid = child.id(),
                udid = udid.unwrap_or("<auto>"),
                "iproxy: spawned tunnel for WDA on port 8100"
            );
            Some(child)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            info!(
                "iproxy not on PATH — install libimobiledevice (or Apple Devices' \
                 ApplicationSupport) if you want WDA macros without manual tunneling. \
                 Set IOS_REMOTE_WDA_URL to override the endpoint."
            );
            None
        }
        Err(e) => {
            warn!(error = %e, "iproxy: spawn failed; macros will require manual tunneling");
            None
        }
    }
}

/// Quick TCP probe: try to connect to `127.0.0.1:8100` for ~200ms. Connect
/// success means the port is already forwarded (existing iproxy or another
/// tunnel). Refused / timeout means we should spawn one ourselves.
fn probe_port_8100() -> bool {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8100);
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_does_not_panic() {
        // The result depends on whether port 8100 happens to be bound during
        // the test run. Either branch is fine; we just guard the function
        // contract: it returns a bool, no panics, no hangs.
        let _ = probe_port_8100();
    }

    #[test]
    fn try_spawn_returns_none_when_iproxy_missing_or_port_bound() {
        // Without iproxy on PATH and without a real device, this should
        // resolve to None within ~200ms. We don't assert truth — CI runners
        // may have iproxy installed — but we do assert it doesn't hang.
        let start = std::time::Instant::now();
        let result = try_spawn(None);
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "try_spawn took too long: {elapsed:?}"
        );
        // Reap the child if one was spawned to avoid zombie processes in CI.
        if let Some(mut child) = result {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
