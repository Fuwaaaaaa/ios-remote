use super::Frame;
use super::http::{HttpRequest, send};
use image::{ImageBuffer, Rgba, imageops::FilterType};
use serde::{Deserialize, Serialize};
use tracing::info;

/// AI screen understanding: send a frame to Claude (Anthropic Messages API)
/// and get a description back.
///
/// Called over raw HTTP via `curl` — there is no official Anthropic SDK for
/// Rust, and the rest of the app already shells out to curl.
const API_URL: &str = "https://api.anthropic.com/v1/messages";
const MODEL: &str = "claude-opus-5";
/// Lets the API re-run a request its safety classifiers decline on
/// Anthropic's recommended fallback model instead of returning a refusal.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Claude accepts images up to 2576px on the long edge; larger ones are
/// scaled down here rather than rejected or silently resampled server-side.
const MAX_LONG_EDGE: u32 = 2576;
/// Base64 inflates by 4/3; keep the encoded image under the API's 5 MB
/// per-image limit.
const MAX_ENCODED_BYTES: usize = 3_700_000;
const DEFAULT_PROMPT: &str =
    "What's shown on this iPhone screen? Describe the app, visible text, and UI state concisely.";

#[derive(Debug, Serialize)]
struct ClaudeRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    fallbacks: &'a str,
    messages: Vec<ClaudeMessage>,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: &'static str,
    content: Vec<ContentBlock>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
}

#[derive(Debug, Serialize)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: &'static str,
    media_type: &'static str,
    data: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    #[serde(default)]
    content: Vec<ResponseContent>,
    stop_reason: Option<String>,
    stop_details: Option<StopDetails>,
}

#[derive(Debug, Deserialize)]
struct ResponseContent {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StopDetails {
    category: Option<String>,
    explanation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorEnvelope {
    error: ApiError,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

/// Whether `ANTHROPIC_API_KEY` is set to a non-empty value.
pub fn api_key_configured() -> bool {
    std::env::var("ANTHROPIC_API_KEY").is_ok_and(|k| !k.trim().is_empty())
}

/// Describe what's on the iPhone screen.
///
/// `prompt` is an optional custom question. Blocking (encodes an image and
/// waits on the network) — call from a blocking context.
pub fn describe_screen(frame: &Frame, prompt: Option<&str>) -> Result<String, String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .ok_or("ANTHROPIC_API_KEY not set. Set it to use AI screen understanding.")?;

    let (media_type, bytes) = encode_for_api(frame)?;
    let question = prompt
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or(DEFAULT_PROMPT);

    let request = ClaudeRequest {
        model: MODEL,
        max_tokens: 16_000,
        fallbacks: "default",
        messages: vec![ClaudeMessage {
            role: "user",
            content: vec![
                ContentBlock::Image {
                    source: ImageSource {
                        source_type: "base64",
                        media_type,
                        data: base64_encode(&bytes),
                    },
                },
                ContentBlock::Text {
                    text: question.to_string(),
                },
            ],
        }],
    };
    let body = serde_json::to_vec(&request).map_err(|e| e.to_string())?;

    let response = send(
        &HttpRequest::new("POST", API_URL)
            .header(format!("x-api-key: {}", api_key.trim()))
            .header("anthropic-version: 2023-06-01")
            .header(format!("anthropic-beta: {FALLBACK_BETA}"))
            .json_body(&body)
            .timeout_secs(180),
    )?;

    let text = parse_response(response.status, &response.body)?;
    info!(chars = text.len(), "AI vision: screen described");
    Ok(text)
}

/// Turn an API response into the description, or an actionable error.
fn parse_response(status: u16, body: &str) -> Result<String, String> {
    if !(200..300).contains(&status) {
        return Err(match serde_json::from_str::<ApiErrorEnvelope>(body) {
            Ok(env) => format!(
                "Claude API error {status} ({}): {}",
                env.error.error_type, env.error.message
            ),
            Err(_) => format!("Claude API error {status}: {}", body.trim()),
        });
    }
    let parsed: ClaudeResponse =
        serde_json::from_str(body).map_err(|e| format!("Failed to parse API response: {e}"))?;

    // A decline is HTTP 200 with stop_reason "refusal" — check it before
    // trusting `content`.
    if parsed.stop_reason.as_deref() == Some("refusal") {
        let details = parsed.stop_details.as_ref();
        let category = details
            .and_then(|d| d.category.as_deref())
            .unwrap_or("unspecified");
        let explanation = details
            .and_then(|d| d.explanation.as_deref())
            .unwrap_or("no explanation provided");
        return Err(format!(
            "Claude declined to describe this screen (category: {category}): {explanation}"
        ));
    }

    // Only visible text: thinking blocks (empty by default) and fallback
    // switch markers are not part of the answer.
    let text = parsed
        .content
        .into_iter()
        .filter(|c| c.block_type == "text")
        .filter_map(|c| c.text)
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return Err(format!(
            "Claude returned no text (stop_reason: {})",
            parsed.stop_reason.as_deref().unwrap_or("unknown")
        ));
    }
    Ok(text)
}

/// Scale the frame to the API's size limit and encode it: PNG for crisp UI
/// text, JPEG only when the PNG would exceed the per-image size limit.
fn encode_for_api(frame: &Frame) -> Result<(&'static str, Vec<u8>), String> {
    let img: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_raw(frame.width, frame.height, frame.rgba.clone())
            .ok_or("Failed to create image buffer")?;
    let (w, h) = scaled_dims(frame.width, frame.height, MAX_LONG_EDGE);
    let img = if (w, h) == (frame.width, frame.height) {
        img
    } else {
        image::imageops::resize(&img, w, h, FilterType::Lanczos3)
    };

    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        img.as_raw(),
        w,
        h,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| format!("PNG encode failed: {e}"))?;
    if png.len() <= MAX_ENCODED_BYTES {
        return Ok(("image/png", png));
    }

    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&rgb)
        .map_err(|e| format!("JPEG encode failed: {e}"))?;
    Ok(("image/jpeg", jpeg))
}

/// Largest size with the same aspect ratio whose long edge is ≤ `max_edge`.
fn scaled_dims(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max_edge || long == 0 {
        return (w, h);
    }
    let scale = max_edge as f64 / long as f64;
    (
        ((w as f64 * scale).round() as u32).max(1),
        ((h as f64 * scale).round() as u32).max(1),
    )
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len() * 4 / 3 + 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_screens_are_scaled_to_the_long_edge_limit() {
        // iPhone 15 Pro Max native screenshot.
        assert_eq!(scaled_dims(1290, 2796, 2576), (1188, 2576));
        assert_eq!(scaled_dims(1170, 2532, 2576), (1170, 2532));
        assert_eq!(scaled_dims(3000, 1000, 2576), (2576, 859));
    }

    #[test]
    fn text_blocks_are_joined_and_thinking_is_skipped() {
        let body = r#"{"content":[
            {"type":"thinking","thinking":""},
            {"type":"text","text":"Settings app"},
            {"type":"text","text":"Wi-Fi is on"}
        ],"stop_reason":"end_turn"}"#;
        assert_eq!(
            parse_response(200, body).unwrap(),
            "Settings app\nWi-Fi is on"
        );
    }

    #[test]
    fn refusal_is_reported_not_returned_as_text() {
        let body = r#"{"content":[],"stop_reason":"refusal",
            "stop_details":{"type":"refusal","category":"cyber","explanation":"nope"}}"#;
        let err = parse_response(200, body).unwrap_err();
        assert!(err.contains("declined") && err.contains("cyber"), "{err}");
    }

    #[test]
    fn api_errors_surface_type_and_message() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        let err = parse_response(401, body).unwrap_err();
        assert!(
            err.contains("401") && err.contains("invalid x-api-key"),
            "{err}"
        );
    }

    #[test]
    fn request_shape_matches_messages_api() {
        let req = ClaudeRequest {
            model: MODEL,
            max_tokens: 16_000,
            fallbacks: "default",
            messages: vec![ClaudeMessage {
                role: "user",
                content: vec![ContentBlock::Text { text: "hi".into() }],
            }],
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "claude-opus-5");
        assert_eq!(json["fallbacks"], "default");
        assert_eq!(json["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
