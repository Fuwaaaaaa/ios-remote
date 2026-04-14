use super::Frame;

/// Screen diff: highlight pixel differences between two consecutive frames.
///
/// Changed pixels are tinted red. Useful for detecting subtle UI changes.

pub fn diff_highlight(prev: &Frame, curr: &Frame, threshold: u32) -> Vec<u8> {
    let w = curr.width;
    let h = curr.height;
    let mut out = curr.rgba.clone();
    let total = (w * h) as usize;

    for i in 0..total {
        let idx = i * 4;
        if idx + 2 >= prev.rgba.len() || idx + 2 >= curr.rgba.len() { break; }

        let dr = (prev.rgba[idx] as i32 - curr.rgba[idx] as i32).unsigned_abs();
        let dg = (prev.rgba[idx + 1] as i32 - curr.rgba[idx + 1] as i32).unsigned_abs();
        let db = (prev.rgba[idx + 2] as i32 - curr.rgba[idx + 2] as i32).unsigned_abs();

        if dr + dg + db > threshold {
            // Tint changed pixel red
            out[idx] = 255;
            out[idx + 1] = (out[idx + 1] as u16 / 3) as u8;
            out[idx + 2] = (out[idx + 2] as u16 / 3) as u8;
        }
    }

    out
}

/// Compute a diff score (0.0 - 1.0) between two frames.
pub fn diff_score(a: &Frame, b: &Frame) -> f64 {
    let total = (a.width * a.height) as usize;
    let mut diff_sum = 0u64;
    for i in 0..total {
        let idx = i * 4;
        if idx + 2 >= a.rgba.len() || idx + 2 >= b.rgba.len() { break; }
        let dr = (a.rgba[idx] as i64 - b.rgba[idx] as i64).unsigned_abs();
        let dg = (a.rgba[idx + 1] as i64 - b.rgba[idx + 1] as i64).unsigned_abs();
        let db = (a.rgba[idx + 2] as i64 - b.rgba[idx + 2] as i64).unsigned_abs();
        diff_sum += dr + dg + db;
    }
    diff_sum as f64 / (total as f64 * 765.0) // normalize to 0-1
}
