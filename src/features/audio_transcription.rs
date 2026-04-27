use std::time::Instant;
use tracing::info;
#[cfg(feature = "whisper")]
use tracing::warn;

/// Process-global Whisper context. Loaded lazily on the first call to
/// [`transcribe_blocking`] inside `tokio::task::spawn_blocking`. `None`
/// means we tried and failed (model missing, init error) — we don't retry.
/// Stored as `Option<Arc<_>>` so each transcription clone-and-uses the
/// context without re-loading the ~140 MB ggml file.
#[cfg(feature = "whisper")]
static WHISPER_CTX: std::sync::OnceLock<Option<std::sync::Arc<whisper_rs::WhisperContext>>> =
    std::sync::OnceLock::new();

/// Audio transcription: real-time speech-to-text from received audio.
///
/// Sends audio to Whisper API (OpenAI) or local whisper.cpp for transcription.
/// Displays subtitles as an overlay on the mirrored screen.
///
/// The capture pump and display thread both consult `now_ms()` so a single
/// monotonic clock anchors subtitle timestamps and visibility windows.
pub struct Transcriber {
    pub enabled: bool,
    pub subtitles: Vec<Subtitle>,
    max_subtitles: usize,
    started: Instant,
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
            started: Instant::now(),
        }
    }

    /// Monotonic milliseconds since this Transcriber was created.
    pub fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    pub fn add_subtitle(&mut self, text: &str, timestamp_ms: u64) {
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

    /// Draw subtitle overlay at bottom of frame using the shared 5x7 bitmap
    /// font from [`super::stats_overlay`]. Long lines are wrapped onto up to
    /// two rows so a 5-second chunk fits in the bar without truncation.
    pub fn draw_subtitles(&self, rgba: &mut [u8], w: u32, h: u32, current_ms: u64) {
        let active = self.active_subtitles(current_ms);
        if active.is_empty() {
            return;
        }

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

        // Use the most recent subtitle; older ones overlap and clutter.
        let line = active.last().map(|s| s.text.as_str()).unwrap_or_default();
        let max_chars = ((w.saturating_sub(16)) / 6).max(1) as usize;
        let (line_a, line_b) = wrap_two_lines(line, max_chars);

        let pad_x = 8u32;
        let row1_y = bar_y + 6;
        let row2_y = bar_y + 22;
        super::stats_overlay::draw_text(
            rgba,
            w,
            pad_x,
            row1_y,
            &line_a,
            super::stats_overlay::COLOR_WHITE,
        );
        if !line_b.is_empty() {
            super::stats_overlay::draw_text(
                rgba,
                w,
                pad_x,
                row2_y,
                &line_b,
                super::stats_overlay::COLOR_WHITE,
            );
        }
    }
}

/// Run a transcription end-to-end without touching any [`Transcriber`]
/// state. Designed to be invoked from `tokio::task::spawn_blocking` since
/// both code paths are synchronous and CPU/IO heavy:
///
/// - **Local whisper.cpp** (when `--features whisper`): clones the cached
///   `WhisperContext` from a process-global `OnceLock`. Loaded exactly
///   once on first call. A `None` cache value means a previous load
///   failed (model missing, init error) and we won't retry.
/// - **OpenAI HTTP fallback**: writes a 16 kHz mono PCM16 WAV to the
///   temp dir and shells out to `curl`. Used when the local path is
///   unavailable or returned an error.
///
/// On success the returned text is the (possibly empty) transcribed
/// chunk. The caller is responsible for storing it on the `Transcriber`
/// via `add_subtitle` — this keeps the heavy work outside the lock so
/// the display loop and `/api/*` handlers aren't blocked during
/// inference.
pub fn transcribe_blocking(
    pcm_16k_mono: &[f32],
    openai_api_key: Option<String>,
) -> Result<String, String> {
    #[cfg(feature = "whisper")]
    {
        let ctx_slot = WHISPER_CTX.get_or_init(|| match load_whisper_context() {
            Ok(ctx) => Some(std::sync::Arc::new(ctx)),
            Err(e) => {
                warn!(error = %e, "whisper context init failed; using OpenAI fallback");
                None
            }
        });
        if let Some(ctx) = ctx_slot {
            match run_whisper(ctx, pcm_16k_mono) {
                Ok(text) => return Ok(text),
                Err(e) => {
                    tracing::debug!(error = %e, "local whisper failed; trying OpenAI API");
                }
            }
        }
    }

    let api_key =
        openai_api_key.ok_or_else(|| "OPENAI_API_KEY not set; cannot transcribe".to_string())?;
    let wav = super::audio_viz::f32_to_wav_bytes(pcm_16k_mono, 16_000, 1);
    // Per-process unique temp file so two concurrent ios-remote instances
    // don't trample each other's uploads.
    let temp = std::env::temp_dir().join(format!("ios_remote_audio_{}.wav", std::process::id()));
    std::fs::write(&temp, &wav).map_err(|e| e.to_string())?;

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
        info!(text = %text, "Audio transcribed (OpenAI)");
    }
    Ok(text)
}

#[cfg(feature = "whisper")]
fn load_whisper_context() -> Result<whisper_rs::WhisperContext, String> {
    use whisper_rs::{WhisperContext, WhisperContextParameters};
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
    WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
        .map_err(|e| format!("whisper init: {e}"))
}

#[cfg(feature = "whisper")]
fn run_whisper(ctx: &whisper_rs::WhisperContext, samples: &[f32]) -> Result<String, String> {
    use whisper_rs::{FullParams, SamplingStrategy};
    let mut state = ctx
        .create_state()
        .map_err(|e| format!("whisper state: {e}"))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_translate(false);
    params.set_print_progress(false);
    params.set_print_special(false);
    state
        .full(params, samples)
        .map_err(|e| format!("whisper run: {e}"))?;

    let n = state.full_n_segments().map_err(|e| e.to_string())?;
    let mut out = String::new();
    for i in 0..n {
        let seg = state.full_get_segment_text(i).map_err(|e| e.to_string())?;
        out.push_str(&seg);
    }
    Ok(out.trim().to_string())
}

/// Split `text` so that each returned line fits within `max_chars`,
/// preferring word boundaries. The output is exactly two lines; long text
/// is truncated with an ellipsis on the second line. If `text` fits on a
/// single line, the second is empty.
fn wrap_two_lines(text: &str, max_chars: usize) -> (String, String) {
    let max = max_chars.max(1);
    if text.chars().count() <= max {
        return (text.to_string(), String::new());
    }

    let break_at = text
        .char_indices()
        .take(max + 1)
        .filter(|(_, c)| c.is_whitespace())
        .last()
        .map(|(i, _)| i)
        .unwrap_or_else(|| {
            text.char_indices()
                .nth(max)
                .map(|(i, _)| i)
                .unwrap_or(text.len())
        });

    let (head, tail) = text.split_at(break_at);
    let head = head.trim_end().to_string();
    let tail = tail.trim_start();
    let mut second: String = tail.chars().take(max).collect();
    if tail.chars().count() > max {
        if second.chars().count() > 1 {
            second = second.chars().take(max - 1).collect();
        }
        second.push('…');
    }
    (head, second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_short_fits_single_line() {
        let (a, b) = wrap_two_lines("hello world", 40);
        assert_eq!(a, "hello world");
        assert_eq!(b, "");
    }

    #[test]
    fn wrap_breaks_on_word_boundary() {
        // First line breaks at the last whitespace within the cap; the second
        // line is truncated with an ellipsis when overflow remains.
        let (a, b) = wrap_two_lines("the quick brown fox jumps", 10);
        assert_eq!(a, "the quick");
        assert!(b.starts_with("brown fox"), "got {b:?}");
    }

    #[test]
    fn wrap_two_lines_fits_exactly() {
        let (a, b) = wrap_two_lines("hello there friend", 12);
        assert_eq!(a, "hello there");
        assert_eq!(b, "friend");
    }

    #[test]
    fn wrap_truncates_with_ellipsis() {
        let text = "aaaaaaaaaa bbbbbbbbbb cccccccccc dddddddddd";
        let (_a, b) = wrap_two_lines(text, 10);
        assert!(b.ends_with('…'));
    }

    #[test]
    fn add_subtitle_drops_oldest_past_cap() {
        let mut t = Transcriber::new();
        t.max_subtitles = 3;
        for i in 0..5 {
            t.add_subtitle(&format!("line {i}"), i * 1000);
        }
        assert_eq!(t.subtitles.len(), 3);
        assert_eq!(t.subtitles[0].text, "line 2");
        assert_eq!(t.subtitles[2].text, "line 4");
    }
}
