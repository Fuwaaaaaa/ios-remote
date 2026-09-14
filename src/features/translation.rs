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

/// Translate text with a LibreTranslate server.
///
/// `LIBRETRANSLATE_URL` points at the `/translate` endpoint (default: the
/// public `https://libretranslate.com/translate`, which requires an API key —
/// set `LIBRETRANSLATE_API_KEY`, or run a local instance and point the URL at
/// it).
fn translate_text(text: &str, source: &str, target: &str) -> Result<String, String> {
    use super::http::{HttpRequest, send};

    let url = std::env::var("LIBRETRANSLATE_URL")
        .unwrap_or_else(|_| "https://libretranslate.com/translate".to_string());
    let mut body = serde_json::json!({
        "q": text,
        "source": source,
        "target": target,
        "format": "text",
    });
    if let Ok(key) = std::env::var("LIBRETRANSLATE_API_KEY")
        && !key.trim().is_empty()
    {
        body["api_key"] = serde_json::Value::String(key.trim().to_string());
    }
    let body = body.to_string();

    let response = send(
        &HttpRequest::new("POST", &url)
            .json_body(body.as_bytes())
            .timeout_secs(30),
    )?;
    parse_translation(response.status, &response.body)
}

/// Extract `translatedText`, or report why there is none. Never echoes the
/// source text back as if it were a translation.
fn parse_translation(status: u16, body: &str) -> Result<String, String> {
    let json: Option<serde_json::Value> = serde_json::from_str(body).ok();
    if !(200..300).contains(&status) {
        let detail = json
            .as_ref()
            .and_then(|j| j["error"].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| body.trim().to_string());
        return Err(format!(
            "translation API error {status}: {detail} (set LIBRETRANSLATE_URL / LIBRETRANSLATE_API_KEY)"
        ));
    }
    json.as_ref()
        .and_then(|j| j["translatedText"].as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "translation API returned no translatedText: {}",
                body.trim()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::parse_translation;

    #[test]
    fn success_returns_translated_text() {
        assert_eq!(
            parse_translation(200, r#"{"translatedText":"こんにちは"}"#).unwrap(),
            "こんにちは"
        );
    }

    #[test]
    fn missing_key_error_is_not_mistaken_for_a_translation() {
        let err = parse_translation(
            400,
            r#"{"error":"Visit https://portal.libretranslate.com to get an API key"}"#,
        )
        .unwrap_err();
        assert!(err.contains("400") && err.contains("API key"), "{err}");
    }

    #[test]
    fn ok_status_without_field_is_an_error() {
        assert!(parse_translation(200, r#"{"unexpected":true}"#).is_err());
    }
}
