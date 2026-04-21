use super::Frame;
use image::{ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use tracing::info;

/// AI screen understanding: send a frame to a vision LLM and get a description.
///
/// Supports multiple backends:
///   1. Claude API (Anthropic) — recommended
///   2. OpenAI Vision API
///   3. Local model (future)

#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeMessage>,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: String,
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
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ResponseContent>,
}

#[derive(Debug, Deserialize)]
struct ResponseContent {
    text: Option<String>,
}

/// Describe what's on the iPhone screen using a vision LLM.
///
/// `api_key` should be set via ANTHROPIC_API_KEY environment variable.
/// `prompt` is an optional custom question (default: "What's shown on this iPhone screen?")
pub fn describe_screen(frame: &Frame, prompt: Option<&str>) -> Result<String, String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "ANTHROPIC_API_KEY not set. Set it to use AI screen understanding.")?;

    // Encode frame as PNG → base64
    let _img: ImageBuffer<Rgba<u8>, _> =
        ImageBuffer::from_raw(frame.width, frame.height, frame.rgba.clone())
            .ok_or("Failed to create image buffer")?;

    let mut png_bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    image::ImageEncoder::write_image(
        encoder,
        &frame.rgba,
        frame.width,
        frame.height,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| format!("PNG encode failed: {}", e))?;

    let b64 = base64_encode(&png_bytes);

    let question = prompt.unwrap_or("What's shown on this iPhone screen? Describe the app, visible text, and UI state concisely.");

    let request = ClaudeRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 500,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: vec![
                ContentBlock::Image {
                    source: ImageSource {
                        source_type: "base64".to_string(),
                        media_type: "image/png".to_string(),
                        data: b64,
                    },
                },
                ContentBlock::Text {
                    text: question.to_string(),
                },
            ],
        }],
    };

    let body = serde_json::to_vec(&request).map_err(|e| e.to_string())?;

    // Synchronous HTTP request (called from non-async context)
    let response = ureq_post(&api_key, &body)?;

    let parsed: ClaudeResponse = serde_json::from_str(&response)
        .map_err(|e| format!("Failed to parse API response: {}", e))?;

    let text = parsed
        .content
        .into_iter()
        .filter_map(|c| c.text)
        .collect::<Vec<_>>()
        .join("\n");

    info!(chars = text.len(), "AI vision: screen described");
    Ok(text)
}

fn ureq_post(api_key: &str, body: &[u8]) -> Result<String, String> {
    // Use std::process::Command with curl as a simple HTTP client
    // (avoids adding another HTTP client dependency)
    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            "https://api.anthropic.com/v1/messages",
            "-H",
            &format!("x-api-key: {}", api_key),
            "-H",
            "anthropic-version: 2023-06-01",
            "-H",
            "content-type: application/json",
            "-d",
            &String::from_utf8_lossy(body),
        ])
        .output()
        .map_err(|e| format!("curl failed: {}", e))?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("API error: {}", stderr))
    }
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
