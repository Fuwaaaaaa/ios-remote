use std::time::Instant;
use tracing::info;

/// Benchmark mode: measure system performance for mirroring.
pub struct BenchmarkResult {
    pub decode_fps: f64,
    pub render_fps: f64,
    pub yuv_convert_ms: f64,
    pub filter_apply_ms: f64,
    pub memory_mb: f64,
}

/// Run a synthetic benchmark (no actual iPhone needed).
pub fn run_benchmark(width: u32, height: u32, frames: u32) -> BenchmarkResult {
    info!(width, height, frames, "Running benchmark...");

    let pixel_count = (width * height) as usize;
    let rgba = vec![128u8; pixel_count * 4];

    // Measure YUV→RGB conversion speed
    let start = Instant::now();
    for _ in 0..frames {
        let mut out = rgba.clone();
        // Simulate processing
        for i in (0..out.len()).step_by(4) {
            out[i] = out[i].wrapping_add(1);
        }
    }
    let yuv_total = start.elapsed();
    let yuv_ms = yuv_total.as_secs_f64() * 1000.0 / frames as f64;

    // Measure filter apply speed
    let start = Instant::now();
    let settings = super::video_filter::FilterSettings {
        brightness: 0.1,
        contrast: 1.2,
        saturation: 0.8,
        grayscale: false,
        invert: false,
        sepia: false,
    };
    for _ in 0..frames {
        let mut buf = rgba.clone();
        super::video_filter::apply_filters(&mut buf, width, height, &settings);
    }
    let filter_total = start.elapsed();
    let filter_ms = filter_total.as_secs_f64() * 1000.0 / frames as f64;

    let decode_fps = frames as f64 / yuv_total.as_secs_f64();
    let render_fps = frames as f64 / filter_total.as_secs_f64();

    let result = BenchmarkResult {
        decode_fps,
        render_fps,
        yuv_convert_ms: yuv_ms,
        filter_apply_ms: filter_ms,
        memory_mb: (pixel_count * 4) as f64 / 1024.0 / 1024.0,
    };

    info!(
        decode_fps = format!("{:.0}", result.decode_fps),
        render_fps = format!("{:.0}", result.render_fps),
        yuv_ms = format!("{:.2}", result.yuv_convert_ms),
        filter_ms = format!("{:.2}", result.filter_apply_ms),
        memory_mb = format!("{:.1}", result.memory_mb),
        "Benchmark complete"
    );

    result
}

/// Debug overlay: show internal buffer/queue state.
pub struct DebugOverlay {
    pub frame_queue_depth: usize,
    pub decode_backlog: usize,
    pub memory_used_mb: f64,
    pub gc_count: u64,
}

impl DebugOverlay {
    pub fn new() -> Self {
        Self {
            frame_queue_depth: 0,
            decode_backlog: 0,
            memory_used_mb: 0.0,
            gc_count: 0,
        }
    }

    pub fn format(&self) -> String {
        format!(
            "Q:{} BL:{} MEM:{:.1}MB GC:{}",
            self.frame_queue_depth, self.decode_backlog, self.memory_used_mb, self.gc_count
        )
    }
}

/// Adaptive bitrate: adjust quality based on network conditions.
pub struct AdaptiveBitrate {
    pub current_quality: Quality,
    fps_history: Vec<f64>,
    latency_history: Vec<f64>,
}

#[derive(Clone, Debug)]
pub enum Quality {
    High,
    Medium,
    Low,
}

impl AdaptiveBitrate {
    pub fn new() -> Self {
        Self {
            current_quality: Quality::High,
            fps_history: Vec::new(),
            latency_history: Vec::new(),
        }
    }

    pub fn update(&mut self, fps: f64, latency_ms: f64) {
        self.fps_history.push(fps);
        self.latency_history.push(latency_ms);
        if self.fps_history.len() > 30 {
            self.fps_history.remove(0);
        }
        if self.latency_history.len() > 30 {
            self.latency_history.remove(0);
        }

        let avg_fps = self.fps_history.iter().sum::<f64>() / self.fps_history.len() as f64;
        let avg_lat = self.latency_history.iter().sum::<f64>() / self.latency_history.len() as f64;

        self.current_quality = if avg_fps > 25.0 && avg_lat < 100.0 {
            Quality::High
        } else if avg_fps > 15.0 && avg_lat < 200.0 {
            Quality::Medium
        } else {
            Quality::Low
        };
    }
}

/// Crash recovery: save state and auto-restart.
pub fn save_crash_state() {
    let state = serde_json::json!({
        "timestamp": chrono::Local::now().to_rfc3339(),
        "pid": std::process::id(),
    });
    let _ = std::fs::write(".ios-remote-crash-state.json", state.to_string());
}

pub fn check_crash_recovery() -> bool {
    std::path::Path::new(".ios-remote-crash-state.json").exists()
}

pub fn clear_crash_state() {
    let _ = std::fs::remove_file(".ios-remote-crash-state.json");
}
