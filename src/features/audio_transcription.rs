use tracing::info;

/// Audio transcription: real-time speech-to-text from received audio.
///
/// Sends audio to Whisper API (OpenAI) or local whisper.cpp for transcription.
/// Displays subtitles as an overlay on the mirrored screen.
pub struct Transcriber {
    pub enabled: bool,
    pub subtitles: Vec<Subtitle>,
    max_subtitles: usize,
}

#[derive(Clone, Debug)]
pub struct Subtitle {
    pub text: String,
    pub start_ms: u64,
    pub duration_ms: u64,
}

impl Transcriber {
    pub fn new() -> Self {
        Self {
            enabled: false,
            subtitles: Vec::new(),
            max_subtitles: 50,
        }
    }

    /// Transcribe a chunk of audio (WAV bytes) using Whisper.
    pub fn transcribe_chunk(
        &mut self,
        wav_data: &[u8],
        timestamp_ms: u64,
    ) -> Result<String, String> {
        // Try local whisper.cpp first; fall through to the API on any error.
        if let Ok(text) = self.try_local_whisper(wav_data) {
            self.add_subtitle(&text, timestamp_ms);
            return Ok(text);
        }

        // Try OpenAI Whisper API
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| "OPENAI_API_KEY not set. Set it for audio transcription.".to_string())?;

        // Save temp WAV file for curl upload
        let temp = std::env::temp_dir().join("ios_remote_audio.wav");
        std::fs::write(&temp, wav_data).map_err(|e| e.to_string())?;

        let output = std::process::Command::new("curl")
            .args([
                "-s",
                "-X",
                "POST",
                "https://api.openai.com/v1/audio/transcriptions",
                "-H",
                &format!("Authorization: Bearer {}", api_key),
                "-F",
                &format!("file=@{}", temp.display()),
                "-F",
                "model=whisper-1",
                "-F",
                "response_format=text",
            ])
            .output()
            .map_err(|e| format!("curl failed: {}", e))?;

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            self.add_subtitle(&text, timestamp_ms);
            info!(text = %text, "Audio transcribed");
        }
        Ok(text)
    }

    /// Transcribe using the `whisper-rs` crate bindings to whisper.cpp.
    /// The model path comes from `IOS_REMOTE_WHISPER_MODEL` (default
    /// `%APPDATA%/ios-remote/models/ggml-base.bin`). Only active when built
    /// with `--features whisper`.
    #[cfg(feature = "whisper")]
    fn try_local_whisper(&self, wav_data: &[u8]) -> Result<String, String> {
        use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

        let model_path = std::env::var("IOS_REMOTE_WHISPER_MODEL").unwrap_or_else(|_| {
            let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
            format!("{appdata}/ios-remote/models/ggml-base.bin")
        });
        if !std::path::Path::new(&model_path).exists() {
            return Err(format!(
                "whisper model not found at {model_path}. \
                 Download ggml-base.bin from https://huggingface.co/ggerganov/whisper.cpp \
                 and set IOS_REMOTE_WHISPER_MODEL."
            ));
        }

        let samples = wav_to_f32(wav_data).map_err(|e| format!("wav decode: {e}"))?;
        let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
            .map_err(|e| format!("whisper init: {e}"))?;
        let mut state = ctx
            .create_state()
            .map_err(|e| format!("whisper state: {e}"))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_translate(false);
        params.set_print_progress(false);
        params.set_print_special(false);
        state
            .full(params, &samples)
            .map_err(|e| format!("whisper run: {e}"))?;

        let n = state.full_n_segments().map_err(|e| e.to_string())?;
        let mut out = String::new();
        for i in 0..n {
            let seg = state.full_get_segment_text(i).map_err(|e| e.to_string())?;
            out.push_str(&seg);
        }
        Ok(out.trim().to_string())
    }

    /// Placeholder that reports the feature is not enabled. When compiled
    /// without `--features whisper` the caller should fall through to the
    /// OpenAI API path.
    #[cfg(not(feature = "whisper"))]
    fn try_local_whisper(&self, _wav_data: &[u8]) -> Result<String, String> {
        Err("whisper feature not enabled (build with --features whisper)".to_string())
    }

    fn add_subtitle(&mut self, text: &str, timestamp_ms: u64) {
        self.subtitles.push(Subtitle {
            text: text.to_string(),
            start_ms: timestamp_ms,
            duration_ms: 3000,
        });
        if self.subtitles.len() > self.max_subtitles {
            self.subtitles.remove(0);
        }
    }

    /// Get active subtitles for the given timestamp.
    pub fn active_subtitles(&self, current_ms: u64) -> Vec<&Subtitle> {
        self.subtitles
            .iter()
            .filter(|s| current_ms >= s.start_ms && current_ms < s.start_ms + s.duration_ms)
            .collect()
    }

    /// Draw subtitle overlay at bottom of frame.
    #[allow(dead_code)]
    pub fn draw_subtitles(&self, rgba: &mut [u8], w: u32, h: u32, current_ms: u64) {
        let active = self.active_subtitles(current_ms);
        if active.is_empty() {
            return;
        }

        // Dark background bar at bottom
        let bar_h = 40u32;
        let bar_y = h.saturating_sub(bar_h);
        for y in bar_y..h {
            for x in 0..w {
                let idx = ((y * w + x) * 4) as usize;
                if idx + 2 < rgba.len() {
                    rgba[idx] /= 3;
                    rgba[idx + 1] /= 3;
                    rgba[idx + 2] /= 3;
                }
            }
        }
        // Text would be drawn with bitmap font (similar to stats_overlay)
    }
}

/// Convert a 16-bit PCM WAV byte slice to normalized `f32` samples in
/// [-1.0, 1.0]. Handles the standard 44-byte header emitted by common capture
/// tools. Returns an error for unsupported bit depths.
#[cfg(feature = "whisper")]
fn wav_to_f32(wav: &[u8]) -> Result<Vec<f32>, String> {
    if wav.len() < 44 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE stream".to_string());
    }
    // Bits per sample is at offset 34 (little-endian u16).
    let bits = u16::from_le_bytes([wav[34], wav[35]]);
    if bits != 16 {
        return Err(format!("only 16-bit PCM supported (got {bits})"));
    }
    let samples = &wav[44..];
    let mut out = Vec::with_capacity(samples.len() / 2);
    for chunk in samples.chunks_exact(2) {
        let s = i16::from_le_bytes([chunk[0], chunk[1]]);
        out.push(s as f32 / 32768.0);
    }
    Ok(out)
}
