use super::Frame;
use tracing::debug;

/// Detect notification banners by analyzing the top region of the frame.
///
/// iOS notification banners appear at the top ~15% of the screen with a
/// distinct visual pattern (rounded rect, blurred background). We detect
/// significant pixel changes in this region between consecutive frames.
pub fn detect_notification_banner(prev: &Frame, curr: &Frame) -> bool {
    if prev.width != curr.width || prev.height != curr.height {
        return false;
    }

    let w = curr.width as usize;
    let h = curr.height as usize;
    let banner_height = h / 7; // top ~14%

    let mut diff_sum: u64 = 0;
    let pixel_count = w * banner_height;

    for y in 0..banner_height {
        for x in 0..w {
            let idx = (y * w + x) * 4;
            if idx + 3 >= prev.rgba.len() || idx + 3 >= curr.rgba.len() {
                break;
            }
            let dr = (prev.rgba[idx] as i32 - curr.rgba[idx] as i32).unsigned_abs();
            let dg = (prev.rgba[idx + 1] as i32 - curr.rgba[idx + 1] as i32).unsigned_abs();
            let db = (prev.rgba[idx + 2] as i32 - curr.rgba[idx + 2] as i32).unsigned_abs();
            diff_sum += (dr + dg + db) as u64;
        }
    }

    let avg_diff = diff_sum / pixel_count.max(1) as u64;
    let detected = avg_diff > 30; // threshold: significant change in banner area

    if detected {
        debug!(avg_diff, "Notification banner detected");
    }

    detected
}

/// Simple motion detection across the full frame.
/// Returns a 0.0-1.0 score of how much the screen changed.
pub fn motion_score(prev: &Frame, curr: &Frame) -> f64 {
    if prev.width != curr.width || prev.height != curr.height {
        return 1.0;
    }

    let total_pixels = (curr.width * curr.height) as usize;
    let mut changed = 0usize;
    let threshold: u32 = 20;

    for i in 0..total_pixels {
        let idx = i * 4;
        if idx + 2 >= prev.rgba.len() || idx + 2 >= curr.rgba.len() {
            break;
        }
        let dr = (prev.rgba[idx] as i32 - curr.rgba[idx] as i32).unsigned_abs();
        let dg = (prev.rgba[idx + 1] as i32 - curr.rgba[idx + 1] as i32).unsigned_abs();
        let db = (prev.rgba[idx + 2] as i32 - curr.rgba[idx + 2] as i32).unsigned_abs();
        if dr + dg + db > threshold {
            changed += 1;
        }
    }

    changed as f64 / total_pixels.max(1) as f64
}
