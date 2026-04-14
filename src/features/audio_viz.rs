use std::collections::VecDeque;

/// Audio waveform visualizer: render real-time waveform/spectrum.
///
/// Receives raw audio samples and generates a visual representation
/// that can be overlaid on the display or shown in a separate panel.

pub struct AudioVisualizer {
    waveform: VecDeque<f32>,
    spectrum: Vec<f32>,
    max_samples: usize,
}

impl AudioVisualizer {
    pub fn new(max_samples: usize) -> Self {
        Self {
            waveform: VecDeque::with_capacity(max_samples),
            spectrum: vec![0.0; 64],
            max_samples,
        }
    }

    /// Push raw PCM samples (mono, -1.0 to 1.0).
    pub fn push_samples(&mut self, samples: &[f32]) {
        for &s in samples {
            self.waveform.push_back(s);
            if self.waveform.len() > self.max_samples {
                self.waveform.pop_front();
            }
        }
        self.compute_spectrum();
    }

    /// Simple DFT for spectrum (64 bands).
    fn compute_spectrum(&mut self) {
        let n = self.waveform.len();
        if n < 128 { return; }
        let samples: Vec<f32> = self.waveform.iter().rev().take(512).copied().collect();
        for k in 0..64 {
            let freq = k as f32 / 64.0 * std::f32::consts::PI;
            let mut real = 0.0f32;
            let mut imag = 0.0f32;
            for (i, &s) in samples.iter().enumerate() {
                let angle = freq * i as f32;
                real += s * angle.cos();
                imag += s * angle.sin();
            }
            self.spectrum[k] = (real * real + imag * imag).sqrt() / samples.len() as f32;
        }
    }

    /// Draw waveform onto an RGBA buffer at given position.
    pub fn draw_waveform(&self, rgba: &mut [u8], buf_w: u32, x: u32, y: u32, w: u32, h: u32) {
        let mid_y = y + h / 2;
        let samples: Vec<f32> = self.waveform.iter().rev().take(w as usize).copied().collect();
        for (i, &s) in samples.iter().enumerate() {
            let px = x + i as u32;
            let py = (mid_y as f32 + s * (h as f32 / 2.0)).clamp(y as f32, (y + h) as f32) as u32;
            if px < buf_w {
                let idx = ((py * buf_w + px) * 4) as usize;
                if idx + 2 < rgba.len() {
                    rgba[idx] = 0; rgba[idx + 1] = 255; rgba[idx + 2] = 128;
                }
            }
        }
    }

    /// Draw spectrum bars onto an RGBA buffer.
    pub fn draw_spectrum(&self, rgba: &mut [u8], buf_w: u32, x: u32, y: u32, w: u32, h: u32) {
        let bar_w = w / 64;
        for (i, &mag) in self.spectrum.iter().enumerate() {
            let bar_h = (mag * h as f32 * 10.0).min(h as f32) as u32;
            let bx = x + i as u32 * bar_w;
            for dy in 0..bar_h {
                for dx in 0..bar_w.saturating_sub(1) {
                    let px = bx + dx;
                    let py = y + h - dy;
                    if px < buf_w {
                        let idx = ((py * buf_w + px) * 4) as usize;
                        if idx + 2 < rgba.len() {
                            let g = (255.0 * (1.0 - dy as f32 / h as f32)) as u8;
                            rgba[idx] = 0; rgba[idx + 1] = g; rgba[idx + 2] = 255;
                        }
                    }
                }
            }
        }
    }
}

/// Record received audio to a WAV file.
pub struct AudioRecorder {
    samples: Vec<i16>,
    sample_rate: u32,
    recording: bool,
}

impl AudioRecorder {
    pub fn new(sample_rate: u32) -> Self {
        Self { samples: Vec::new(), sample_rate, recording: false }
    }

    pub fn start(&mut self) { self.samples.clear(); self.recording = true; }
    pub fn stop(&mut self) { self.recording = false; }

    pub fn push(&mut self, pcm_i16: &[i16]) {
        if self.recording { self.samples.extend_from_slice(pcm_i16); }
    }

    /// Save as WAV file (PCM 16-bit mono).
    pub fn save_wav(&self, path: &str) -> Result<(), String> {
        let data_size = (self.samples.len() * 2) as u32;
        let file_size = 36 + data_size;
        let mut buf = Vec::with_capacity(file_size as usize + 8);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&1u16.to_le_bytes()); // mono
        buf.extend_from_slice(&self.sample_rate.to_le_bytes());
        buf.extend_from_slice(&(self.sample_rate * 2).to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes()); // block align
        buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        for &s in &self.samples { buf.extend_from_slice(&s.to_le_bytes()); }
        std::fs::write(path, buf).map_err(|e| e.to_string())
    }
}
