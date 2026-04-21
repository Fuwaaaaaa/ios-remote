use tracing::info;

/// Imgur instant share: upload screenshot and copy URL to clipboard.
///
/// One-key workflow: capture frame → PNG → upload to Imgur → URL in clipboard.
pub fn upload_to_imgur(png_path: &str) -> Result<String, String> {
    let client_id = std::env::var("IMGUR_CLIENT_ID").unwrap_or_else(|_| "anonymous".to_string());

    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            "https://api.imgur.com/3/image",
            "-H",
            &format!("Authorization: Client-ID {}", client_id),
            "-F",
            &format!("image=@{}", png_path),
        ])
        .output()
        .map_err(|e| format!("curl failed: {}", e))?;

    let resp: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("JSON parse error: {}", e))?;

    if let Some(link) = resp["data"]["link"].as_str() {
        // Copy URL to clipboard
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard.set_text(link).map_err(|e| e.to_string())?;
        info!(url = %link, "Screenshot uploaded to Imgur → clipboard");
        Ok(link.to_string())
    } else {
        let error = resp["data"]["error"].as_str().unwrap_or("unknown error");
        Err(format!("Imgur upload failed: {}", error))
    }
}

/// Quick share: screenshot → Imgur → clipboard URL, all in one call.
pub fn quick_share(bus: &super::FrameBus) -> Result<String, String> {
    let frame = bus.latest_frame().ok_or("No frame")?;
    let path = super::screenshot::save_frame(&frame)?;
    upload_to_imgur(&path)
}
