use crate::config::ConnectionHistory;
use tracing::info;

/// Auto-connect: automatically reconnect to previously known devices.
///
/// Checks the connection history and monitors mDNS for known device IDs.
/// When a known device appears on the network, triggers auto-pairing.
pub struct AutoConnect {
    known_devices: Vec<String>,
    enabled: bool,
}

impl AutoConnect {
    pub fn new(history: &ConnectionHistory) -> Self {
        let known = history
            .records
            .iter()
            .map(|r| r.device_id.clone())
            .collect();

        Self {
            known_devices: known,
            enabled: true,
        }
    }

    /// Check if a discovered device should be auto-connected.
    pub fn should_auto_connect(&self, device_id: &str) -> bool {
        self.enabled && self.known_devices.contains(&device_id.to_string())
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        info!(enabled = self.enabled, "Auto-connect toggled");
    }
}

/// Windows firewall: add rules for ios-remote ports.
///
/// Requires administrator privileges. Adds inbound rules for:
///   - RTSP (7000/TCP)
///   - NTP (7010/UDP)
///   - Video (7100/TCP)
///   - Event (7200/TCP)
///   - Web (8080/TCP)
pub fn configure_firewall() -> Result<(), String> {
    let rules = [
        ("ios-remote RTSP", "7000", "TCP"),
        ("ios-remote NTP", "7010", "UDP"),
        ("ios-remote Video", "7100", "TCP"),
        ("ios-remote Event", "7200", "TCP"),
        ("ios-remote Web", "8080", "TCP"),
    ];

    for (name, port, protocol) in &rules {
        let output = std::process::Command::new("netsh")
            .args([
                "advfirewall",
                "firewall",
                "add",
                "rule",
                &format!("name={}", name),
                "dir=in",
                "action=allow",
                &format!("protocol={}", protocol),
                &format!("localport={}", port),
            ])
            .output()
            .map_err(|e| format!("netsh failed: {}. Run as Administrator.", e))?;

        if output.status.success() {
            info!(name = %name, port = %port, "Firewall rule added");
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to add rule {}: {}", name, err));
        }
    }

    Ok(())
}

/// Performance graph data point.
#[derive(Clone, Debug)]
pub struct PerfSample {
    pub timestamp_ms: u64,
    pub fps: f64,
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub bandwidth_kbps: f64,
}

/// Performance tracker: collect metrics over time.
pub struct PerfTracker {
    pub samples: Vec<PerfSample>,
    max_samples: usize,
}

impl PerfTracker {
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: Vec::new(),
            max_samples,
        }
    }

    pub fn push(&mut self, sample: PerfSample) {
        self.samples.push(sample);
        if self.samples.len() > self.max_samples {
            self.samples.remove(0);
        }
    }

    pub fn avg_fps(&self, last_n: usize) -> f64 {
        let recent: Vec<_> = self.samples.iter().rev().take(last_n).collect();
        if recent.is_empty() {
            return 0.0;
        }
        recent.iter().map(|s| s.fps).sum::<f64>() / recent.len() as f64
    }
}
