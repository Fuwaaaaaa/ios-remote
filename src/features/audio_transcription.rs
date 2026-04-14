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
        Self { enabled: false, subtitles: Vec::new(), max_subtitles: 50 }
    }

    /// Transcribe a chunk of audio (WAV bytes) using Whisper.
    pub fn transcribe_chunk(&mut self, wav_data: &[u8], timestamp_ms: u64) -> Result<String, String> {
        // Try local whisper.cpp first
        match self.try_local_whisper(wav_data) {
            Ok(text) => {
                self.add_subtitle(&text, timestamp_ms);
                return Ok(text);
            }
            Err(_) => {} // fall through to API
        }

        // Try OpenAI Whisper API
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| "OPENAI_API_KEY not set. Set it for audio transcription.".to_string())?;

        // Save temp WAV file for curl upload
        let temp = std::env::temp_dir().join("ios_remote_audio.wav");
        std::fs::write(&temp, wav_data).map_err(|e| e.to_string())?;

        let output = std::process::Command::new("curl")
            .args([
                "-s", "-X", "POST",
                "https://api.openai.com/v1/audio/transcriptions",
                "-H", &format!("Authorization: Bearer {}", api_key),
                "-F", &format!("file=@{}", temp.display()),
                "-F", "model=whisper-1",
                "-F", "response_format=text",
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

    fn try_local_whisper(&self, _wav_data: &[u8]) -> Result<String, String> {
        // Check if whisper.cpp CLI is available
        let _output = std::process::Command::new("whisper")
            .arg("--help")
            .output()
            .map_err(|_| "whisper CLI not found")?;
        Err("Local whisper not yet integrated".to_string())
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
        self.subtitles.iter().filter(|s| {
            current_ms >= s.start_ms && current_ms < s.start_ms + s.duration_ms
        }).collect()
    }

    /// Draw subtitle overlay at bottom of frame.
    pub fn draw_subtitles(&self, rgba: &mut [u8], w: u32, h: u32, current_ms: u64) {
        let active = self.active_subtitles(current_ms);
        if active.is_empty() { return; }

        // Dark background bar at bottom
        let bar_h = 40u32;
        let bar_y = h.saturating_sub(bar_h);
        for y in bar_y..h {
            for x in 0..w {
                let idx = ((y * w + x) * 4) as usize;
                if idx + 2 < rgba.len() {
                    rgba[idx] = rgba[idx] / 3;
                    rgba[idx + 1] = rgba[idx + 1] / 3;
                    rgba[idx + 2] = rgba[idx + 2] / 3;
                }
            }
        }
        // Text would be drawn with bitmap font (similar to stats_overlay)
    }
}
