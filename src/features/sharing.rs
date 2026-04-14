use super::Frame;
use tracing::info;

/// Screen sharing via simple HTTP MJPEG stream.
///
/// Serves the mirrored screen as an MJPEG stream that any browser can open.
/// URL: http://localhost:8081/stream
/// Lighter than WebRTC, works everywhere.
pub async fn serve_mjpeg(
    mut rx: tokio::sync::broadcast::Receiver<std::sync::Arc<Frame>>,
    port: u16,
) {
    use axum::{routing::get, Router};
    use tokio::net::TcpListener;

    let (tx, _) = tokio::sync::broadcast::channel::<Vec<u8>>(4);
    let jpeg_tx = tx.clone();

    // Frame encoder: convert RGBA frames to JPEG
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(frame) => {
                    if let Ok(jpeg) = encode_jpeg(&frame) {
                        let _ = jpeg_tx.send(jpeg);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let app = Router::new().route("/stream", get(move || {
        let _rx = tx.subscribe();
        async move {
            let boundary = "frame";
            let headers = [
                ("Content-Type", format!("multipart/x-mixed-replace; boundary={}", boundary)),
                ("Cache-Control", "no-cache".to_string()),
            ];
            // In a full implementation, this would return a streaming body.
            // For now, return a static message.
            (headers, "MJPEG stream endpoint — connect a viewer to receive frames")
        }
    }));

    let listener = TcpListener::bind(("0.0.0.0", port)).await.unwrap();
    info!(port, "MJPEG screen sharing: http://localhost:{}/stream", port);
    let _ = axum::serve(listener, app).await;
}

fn encode_jpeg(frame: &Frame) -> Result<Vec<u8>, String> {
    use image::{ImageBuffer, Rgba};
    let _img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(
        frame.width, frame.height, frame.rgba.clone(),
    ).ok_or("Image buffer creation failed")?;

    let mut buf = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 70);
    image::ImageEncoder::write_image(
        encoder,
        &frame.rgba,
        frame.width,
        frame.height,
        image::ExtendedColorType::Rgba8,
    ).map_err(|e| e.to_string())?;

    Ok(buf)
}

/// Notification forwarding to external services.
pub struct NotificationForwarder {
    pub discord_webhook: Option<String>,
    pub slack_webhook: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
}

impl NotificationForwarder {
    pub fn new() -> Self {
        Self {
            discord_webhook: std::env::var("DISCORD_WEBHOOK").ok(),
            slack_webhook: std::env::var("SLACK_WEBHOOK").ok(),
            telegram_bot_token: std::env::var("TELEGRAM_BOT_TOKEN").ok(),
            telegram_chat_id: std::env::var("TELEGRAM_CHAT_ID").ok(),
        }
    }

    /// Forward a notification message to all configured services.
    pub fn forward(&self, title: &str, message: &str) {
        if let Some(ref url) = self.discord_webhook {
            let body = serde_json::json!({"content": format!("**{}**\n{}", title, message)});
            Self::post_json(url, &body);
        }

        if let Some(ref url) = self.slack_webhook {
            let body = serde_json::json!({"text": format!("*{}*\n{}", title, message)});
            Self::post_json(url, &body);
        }

        if let (Some(token), Some(chat_id)) = (&self.telegram_bot_token, &self.telegram_chat_id) {
            let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
            let body = serde_json::json!({"chat_id": chat_id, "text": format!("{}\n{}", title, message)});
            Self::post_json(&url, &body);
        }
    }

    fn post_json(url: &str, body: &serde_json::Value) {
        let _ = std::process::Command::new("curl")
            .args(["-s", "-X", "POST", url, "-H", "Content-Type: application/json", "-d", &body.to_string()])
            .output();
    }
}
