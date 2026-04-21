use chrono::Local;
use serde::Serialize;
use tracing::info;

/// Session statistics export: CSV/JSON export of all metrics.

#[derive(Clone, Debug, Serialize)]
pub struct StatsSample {
    pub timestamp: String,
    pub fps: f64,
    pub latency_ms: f64,
    pub bandwidth_kbps: f64,
    pub frames_total: u64,
    pub cpu_percent: f64,
    pub memory_mb: f64,
}

pub struct StatsExporter {
    samples: Vec<StatsSample>,
}

impl StatsExporter {
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
        }
    }

    pub fn push(
        &mut self,
        fps: f64,
        latency: f64,
        bandwidth: f64,
        frames: u64,
        cpu: f64,
        mem: f64,
    ) {
        self.samples.push(StatsSample {
            timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            fps,
            latency_ms: latency,
            bandwidth_kbps: bandwidth,
            frames_total: frames,
            cpu_percent: cpu,
            memory_mb: mem,
        });
    }

    pub fn export_csv(&self, path: &str) -> Result<(), String> {
        let mut csv =
            "timestamp,fps,latency_ms,bandwidth_kbps,frames_total,cpu_percent,memory_mb\n"
                .to_string();
        for s in &self.samples {
            csv.push_str(&format!(
                "{},{:.1},{:.1},{:.1},{},{:.1},{:.1}\n",
                s.timestamp,
                s.fps,
                s.latency_ms,
                s.bandwidth_kbps,
                s.frames_total,
                s.cpu_percent,
                s.memory_mb
            ));
        }
        std::fs::write(path, csv).map_err(|e| e.to_string())?;
        info!(path, samples = self.samples.len(), "Stats exported to CSV");
        Ok(())
    }

    pub fn export_json(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.samples).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())?;
        info!(path, "Stats exported to JSON");
        Ok(())
    }

    /// Generate EDL (Edit Decision List) for video editors.
    pub fn export_edl(&self, path: &str, session_name: &str) -> Result<(), String> {
        let mut edl = format!("TITLE: {}\nFCM: NON-DROP FRAME\n\n", session_name);
        for (i, _s) in self.samples.iter().enumerate() {
            let tc = format!("{:02}:{:02}:{:02}:00", i / 3600, (i % 3600) / 60, i % 60);
            edl.push_str(&format!(
                "{:03}  AX  V  C  {} {} {} {}\n",
                i + 1,
                tc,
                tc,
                tc,
                tc
            ));
        }
        std::fs::write(path, edl).map_err(|e| e.to_string())?;
        info!(path, "EDL timeline exported");
        Ok(())
    }
}
