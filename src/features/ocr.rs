use super::Frame;
use image::{ImageBuffer, Rgba};
use tracing::info;

/// OCR: extract text from the mirrored iPhone screen.
///
/// Captures the current frame, optionally crops a region, and extracts
/// visible text. Useful for copying text from iPhone without touching it.
///
/// Two backends:
///   1. Tesseract (local, requires tesseract-ocr installed)
///   2. Cloud API (sends image to an OCR API endpoint)

/// Extract text from a frame region.
///
/// `region` is (x, y, width, height) in pixels. Pass None for full frame.
pub fn extract_text(frame: &Frame, region: Option<(u32, u32, u32, u32)>) -> Result<String, String> {
    let (crop_rgba, crop_w, crop_h) = match region {
        Some((rx, ry, rw, rh)) => {
            crop_frame(frame, rx, ry, rw, rh)?
        }
        None => {
            (frame.rgba.clone(), frame.width, frame.height)
        }
    };

    // Save cropped region as temp PNG for OCR
    let img: ImageBuffer<Rgba<u8>, _> = ImageBuffer::from_raw(crop_w, crop_h, crop_rgba)
        .ok_or("Failed to create image buffer")?;

    let temp_path = std::env::temp_dir().join("ios_remote_ocr.png");
    img.save(&temp_path).map_err(|e| e.to_string())?;

    // Try tesseract CLI
    match run_tesseract(&temp_path) {
        Ok(text) => {
            info!(chars = text.len(), "OCR extracted text");
            Ok(text)
        }
        Err(e) => {
            Err(format!("OCR failed: {}. Is tesseract-ocr installed? \
                        Install: https://github.com/tesseract-ocr/tesseract", e))
        }
    }
}

fn run_tesseract(image_path: &std::path::Path) -> Result<String, String> {
    let output = std::process::Command::new("tesseract")
        .args([
            image_path.to_str().unwrap_or(""),
            "stdout",
            "-l", "eng+jpn", // English + Japanese
            "--psm", "3",    // Fully automatic page segmentation
        ])
        .output()
        .map_err(|e| format!("Failed to run tesseract: {}", e))?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .map(|s| s.trim().to_string())
            .map_err(|e| e.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("tesseract error: {}", stderr))
    }
}

fn crop_frame(frame: &Frame, x: u32, y: u32, w: u32, h: u32) -> Result<(Vec<u8>, u32, u32), String> {
    let fw = frame.width;
    let fh = frame.height;

    if x + w > fw || y + h > fh {
        return Err(format!("Crop region ({},{} {}x{}) exceeds frame ({}x{})", x, y, w, h, fw, fh));
    }

    let mut cropped = Vec::with_capacity((w * h * 4) as usize);
    for row in y..(y + h) {
        let start = ((row * fw + x) * 4) as usize;
        let end = start + (w * 4) as usize;
        if end <= frame.rgba.len() {
            cropped.extend_from_slice(&frame.rgba[start..end]);
        }
    }

    Ok((cropped, w, h))
}
