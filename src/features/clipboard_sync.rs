use super::FrameBus;
use arboard::Clipboard;
use tracing::info;

/// Clipboard sync: OCR text from iPhone screen → PC clipboard.
///
/// Monitors frames, and when triggered (via hotkey C or API),
/// runs OCR on the current frame and copies the result to the PC clipboard.
pub fn copy_screen_text_to_clipboard(bus: &FrameBus) -> Result<String, String> {
    let frame = bus.latest_frame().ok_or("No frame available")?;
    let text = super::ocr::extract_text(&frame, None)?;

    if text.is_empty() {
        return Err("No text detected on screen".to_string());
    }

    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {}", e))?;
    clipboard
        .set_text(&text)
        .map_err(|e| format!("Clipboard set error: {}", e))?;

    info!(chars = text.len(), "Screen text copied to clipboard");
    Ok(text)
}

/// Copy the latest screenshot image to clipboard.
pub fn copy_screenshot_to_clipboard(bus: &FrameBus) -> Result<(), String> {
    let frame = bus.latest_frame().ok_or("No frame available")?;

    let mut clipboard = Clipboard::new().map_err(|e| format!("Clipboard error: {}", e))?;
    let img_data = arboard::ImageData {
        width: frame.width as usize,
        height: frame.height as usize,
        bytes: std::borrow::Cow::Borrowed(&frame.rgba),
    };
    clipboard
        .set_image(img_data)
        .map_err(|e| format!("Clipboard image error: {}", e))?;

    info!("Screenshot copied to clipboard");
    Ok(())
}
