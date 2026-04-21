use super::Frame;
use tracing::info;

/// Translation overlay: extract text via OCR, translate, and overlay on frame.
///
/// Uses OCR to extract text regions, then translates via API and draws
/// translated text over the original position.
pub struct TranslationOverlay {
    pub source_lang: String,
    pub target_lang: String,
    pub enabled: bool,
    cached_translations: Vec<TranslatedRegion>,
}

#[derive(Clone, Debug)]
struct TranslatedRegion {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub original: String,
    pub translated: String,
}

impl TranslationOverlay {
    pub fn new(source: &str, target: &str) -> Self {
        Self {
            source_lang: source.to_string(),
            target_lang: target.to_string(),
            enabled: false,
            cached_translations: Vec::new(),
        }
    }

    /// Translate visible text on the frame.
    /// Returns translated text pairs.
    pub fn translate_frame(&mut self, frame: &Frame) -> Result<Vec<(String, String)>, String> {
        // Step 1: OCR the full frame
        let text = super::ocr::extract_text(frame, None)?;

        if text.is_empty() {
            return Ok(vec![]);
        }

        // Step 2: Translate via API (using curl to a translation service)
        let translated = translate_text(&text, &self.source_lang, &self.target_lang)?;

        let pairs: Vec<(String, String)> = text
            .lines()
            .zip(translated.lines())
            .map(|(o, t)| (o.to_string(), t.to_string()))
            .collect();

        info!(
            pairs = pairs.len(),
            from = %self.source_lang,
            to = %self.target_lang,
            "Translation complete"
        );

        Ok(pairs)
    }
}

/// Translate text using a free translation API (LibreTranslate or similar).
fn translate_text(text: &str, source: &str, target: &str) -> Result<String, String> {
    // Try LibreTranslate (self-hosted or public instance)
    let body = serde_json::json!({
        "q": text,
        "source": source,
        "target": target,
    });

    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            "https://libretranslate.com/translate",
            "-H",
            "Content-Type: application/json",
            "-d",
            &body.to_string(),
        ])
        .output()
        .map_err(|e| format!("curl failed: {}", e))?;

    if output.status.success() {
        let resp: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("JSON parse error: {}", e))?;
        Ok(resp["translatedText"].as_str().unwrap_or(text).to_string())
    } else {
        // Fallback: return original text
        Err("Translation API unavailable — install LibreTranslate locally".to_string())
    }
}
