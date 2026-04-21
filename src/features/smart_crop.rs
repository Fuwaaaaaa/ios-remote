use super::Frame;

/// Smart crop: auto-detect content boundaries and crop the frame.
///
/// Finds the bounding box of non-background content. Useful when the
/// iPhone shows letterboxed content (e.g., portrait app on landscape screen).
pub struct CropResult {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub cropped_rgba: Vec<u8>,
}

/// Detect content boundaries and crop.
/// `bg_threshold`: how close a pixel must be to the border color to be "background".
pub fn smart_crop(frame: &Frame, bg_threshold: u32) -> CropResult {
    let w = frame.width;
    let h = frame.height;

    // Sample border color from corners
    let bg_color = sample_border_color(&frame.rgba, w, h);

    // Find bounding box of non-background content
    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            if idx + 2 >= frame.rgba.len() { continue; }

            let dr = (frame.rgba[idx] as i32 - bg_color[0] as i32).unsigned_abs();
            let dg = (frame.rgba[idx + 1] as i32 - bg_color[1] as i32).unsigned_abs();
            let db = (frame.rgba[idx + 2] as i32 - bg_color[2] as i32).unsigned_abs();

            if dr + dg + db > bg_threshold {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    // Fallback to full frame if no content detected
    if max_x <= min_x || max_y <= min_y {
        return CropResult {
            x: 0, y: 0, w, h,
            cropped_rgba: frame.rgba.clone(),
        };
    }

    let crop_w = max_x - min_x + 1;
    let crop_h = max_y - min_y + 1;

    let mut cropped = Vec::with_capacity((crop_w * crop_h * 4) as usize);
    for cy in min_y..=max_y {
        let start = ((cy * w + min_x) * 4) as usize;
        let end = start + (crop_w * 4) as usize;
        if end <= frame.rgba.len() {
            cropped.extend_from_slice(&frame.rgba[start..end]);
        }
    }

    CropResult {
        x: min_x, y: min_y, w: crop_w, h: crop_h,
        cropped_rgba: cropped,
    }
}

fn sample_border_color(rgba: &[u8], w: u32, h: u32) -> [u8; 3] {
    // Average the four corners (5x5 each)
    let mut sum = [0u64; 3];
    let mut count = 0u64;

    for &(bx, by) in &[(0, 0), (w - 5, 0), (0, h - 5), (w - 5, h - 5)] {
        for dy in 0..5u32 {
            for dx in 0..5u32 {
                let idx = (((by + dy) * w + (bx + dx)) * 4) as usize;
                if idx + 2 < rgba.len() {
                    sum[0] += rgba[idx] as u64;
                    sum[1] += rgba[idx + 1] as u64;
                    sum[2] += rgba[idx + 2] as u64;
                    count += 1;
                }
            }
        }
    }

    if count == 0 { return [0, 0, 0]; }
    [(sum[0] / count) as u8, (sum[1] / count) as u8, (sum[2] / count) as u8]
}
