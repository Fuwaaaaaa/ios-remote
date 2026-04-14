use std::time::{Duration, Instant};

/// Bandwidth throttle for controlling network usage.
///
/// Can be applied to the video stream receiver to limit bandwidth consumption.

pub struct Throttle {
    limit_bytes_per_sec: u64,
    window_bytes: u64,
    window_start: Instant,
}

impl Throttle {
    /// Create a throttle with limit in kilobits per second.
    pub fn new_kbps(kbps: u64) -> Self {
        Self {
            limit_bytes_per_sec: kbps * 1024 / 8,
            window_bytes: 0,
            window_start: Instant::now(),
        }
    }

    /// Returns how long to sleep before accepting `bytes` more data.
    /// Returns Duration::ZERO if under limit.
    pub fn delay_for(&mut self, bytes: u64) -> Duration {
        let elapsed = self.window_start.elapsed();

        if elapsed >= Duration::from_secs(1) {
            // Reset window
            self.window_bytes = 0;
            self.window_start = Instant::now();
        }

        self.window_bytes += bytes;

        if self.window_bytes > self.limit_bytes_per_sec {
            // Calculate how long to wait
            let overshoot = self.window_bytes - self.limit_bytes_per_sec;
            let wait_secs = overshoot as f64 / self.limit_bytes_per_sec as f64;
            Duration::from_secs_f64(wait_secs)
        } else {
            Duration::ZERO
        }
    }

    /// Check if currently throttled.
    pub fn is_throttled(&self) -> bool {
        self.window_bytes > self.limit_bytes_per_sec
    }

    /// Current usage as a fraction (0.0 - 1.0+).
    pub fn usage_fraction(&self) -> f64 {
        if self.limit_bytes_per_sec == 0 { return 0.0; }
        self.window_bytes as f64 / self.limit_bytes_per_sec as f64
    }
}
