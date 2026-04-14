use super::Frame;
use image::GrayImage;
use tracing::info;

/// QR/barcode scanner: detect and decode QR codes from the mirrored screen.
///
/// Scans the current frame for QR codes and returns decoded strings.
/// Uses the `rqrr` crate for pure-Rust QR detection.
pub fn scan_qr_codes(frame: &Frame) -> Vec<String> {
    if frame.rgba.is_empty() || frame.width == 0 || frame.height == 0 {
        return vec![];
    }

    // Convert RGBA to grayscale
    let gray = rgba_to_gray(&frame.rgba, frame.width, frame.height);

    // Prepare for rqrr
    let img = GrayImage::from_raw(frame.width, frame.height, gray)
        .unwrap_or_else(|| GrayImage::new(1, 1));

    let mut prepared = rqrr::PreparedImage::prepare(img);
    let grids = prepared.detect_grids();

    let mut results = Vec::new();
    for grid in grids {
        match grid.decode() {
            Ok((_meta, content)) => {
                info!(content = %content, "QR code detected");
                results.push(content);
            }
            Err(e) => {
                tracing::debug!(error = %e, "QR grid decode failed");
            }
        }
    }

    results
}

/// Scan for QR codes and copy first result to clipboard.
pub fn scan_and_copy(frame: &Frame) -> Result<String, String> {
    let codes = scan_qr_codes(frame);
    if codes.is_empty() {
        return Err("No QR code detected on screen".to_string());
    }

    let text = &codes[0];

    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;

    info!(text = %text, "QR code copied to clipboard");
    Ok(text.clone())
}

fn rgba_to_gray(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let count = (width * height) as usize;
    let mut gray = Vec::with_capacity(count);
    for i in 0..count {
        let idx = i * 4;
        if idx + 2 < rgba.len() {
            let r = rgba[idx] as u32;
            let g = rgba[idx + 1] as u32;
            let b = rgba[idx + 2] as u32;
            gray.push(((r * 299 + g * 587 + b * 114) / 1000) as u8);
        } else {
            gray.push(0);
        }
    }
    gray
}
