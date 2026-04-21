use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tracing::info;

/// Network diagnostics: measure connection quality to iPhone.

#[derive(Debug, Clone, Serialize)]
pub struct NetworkStats {
    pub ping_ms: f64,
    pub jitter_ms: f64,
    pub packet_loss_percent: f64,
    pub bandwidth_estimate_mbps: f64,
}

/// Measure round-trip latency to a target IP using UDP echo.
pub async fn measure_latency(target_ip: &str, count: u32) -> Result<NetworkStats, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| e.to_string())?;
    let target = format!("{}:7010", target_ip); // NTP port as echo target

    let mut rtts = Vec::new();
    let mut lost = 0u32;

    for i in 0..count {
        let payload = format!("PING:{}", i);
        let start = Instant::now();

        socket
            .send_to(payload.as_bytes(), &target)
            .await
            .map_err(|e| e.to_string())?;

        let mut buf = [0u8; 64];
        match tokio::time::timeout(Duration::from_millis(1000), socket.recv_from(&mut buf)).await {
            Ok(Ok(_)) => {
                let rtt = start.elapsed().as_secs_f64() * 1000.0;
                rtts.push(rtt);
            }
            _ => {
                lost += 1;
            }
        }
    }

    if rtts.is_empty() {
        return Err("All packets lost — device unreachable".to_string());
    }

    let avg_ping = rtts.iter().sum::<f64>() / rtts.len() as f64;

    let jitter = if rtts.len() > 1 {
        let diffs: Vec<f64> = rtts.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
        diffs.iter().sum::<f64>() / diffs.len() as f64
    } else {
        0.0
    };

    let loss = (lost as f64 / count as f64) * 100.0;

    let stats = NetworkStats {
        ping_ms: avg_ping,
        jitter_ms: jitter,
        packet_loss_percent: loss,
        bandwidth_estimate_mbps: 0.0, // requires separate bandwidth test
    };

    info!(
        ping = format!("{:.1}ms", stats.ping_ms),
        jitter = format!("{:.1}ms", stats.jitter_ms),
        loss = format!("{:.1}%", stats.packet_loss_percent),
        "Network diagnostics"
    );

    Ok(stats)
}

/// Bandwidth throttling: limit the receive rate.
pub struct BandwidthThrottle {
    pub limit_kbps: u64,
    bytes_this_second: u64,
    last_reset: Instant,
}

impl BandwidthThrottle {
    pub fn new(limit_kbps: u64) -> Self {
        Self {
            limit_kbps,
            bytes_this_second: 0,
            last_reset: Instant::now(),
        }
    }

    /// Check if we should accept more data. Returns true if under limit.
    pub fn allow(&mut self, bytes: u64) -> bool {
        if self.last_reset.elapsed() >= Duration::from_secs(1) {
            self.bytes_this_second = 0;
            self.last_reset = Instant::now();
        }

        let limit_bytes = self.limit_kbps * 1024 / 8;
        if self.bytes_this_second + bytes <= limit_bytes {
            self.bytes_this_second += bytes;
            true
        } else {
            false // throttled
        }
    }

    pub fn usage_percent(&self) -> f64 {
        let limit_bytes = (self.limit_kbps * 1024 / 8) as f64;
        if limit_bytes == 0.0 {
            return 0.0;
        }
        (self.bytes_this_second as f64 / limit_bytes) * 100.0
    }
}
