use super::Frame;

/// Template matching: detect when a specific screen/element appears.
///
/// Used by the macro system's WaitForScreen action and for
/// auto-triggering actions when specific UI states are detected.

#[derive(Clone, Debug)]
pub struct MatchResult {
    pub x: u32,
    pub y: u32,
    pub score: f64,
    pub matched: bool,
}

/// Compare a template image against a region of the current frame.
///
/// Uses normalized cross-correlation (NCC) for matching.
/// `threshold` is 0.0-1.0 (0.85 is a good default for exact matches).
pub fn find_template(
    frame: &Frame,
    template_rgba: &[u8],
    template_w: u32,
    template_h: u32,
    region: Option<(u32, u32, u32, u32)>,
    threshold: f64,
) -> MatchResult {
    let (search_x, search_y, search_w, search_h) =
        region.unwrap_or((0, 0, frame.width, frame.height));

    let mut best_score = 0.0f64;
    let mut best_x = 0u32;
    let mut best_y = 0u32;

    let step = 2; // Skip every 2 pixels for speed

    for sy in (search_y..search_y + search_h - template_h).step_by(step) {
        for sx in (search_x..search_x + search_w - template_w).step_by(step) {
            let score = ncc_score(
                &frame.rgba,
                frame.width,
                template_rgba,
                template_w,
                template_h,
                sx,
                sy,
            );

            if score > best_score {
                best_score = score;
                best_x = sx;
                best_y = sy;

                // Early exit if perfect match
                if score > 0.98 {
                    return MatchResult {
                        x: best_x,
                        y: best_y,
                        score: best_score,
                        matched: true,
                    };
                }
            }
        }
    }

    MatchResult {
        x: best_x,
        y: best_y,
        score: best_score,
        matched: best_score >= threshold,
    }
}

/// Normalized cross-correlation between template and frame region.
fn ncc_score(
    frame: &[u8],
    frame_w: u32,
    template: &[u8],
    t_w: u32,
    t_h: u32,
    offset_x: u32,
    offset_y: u32,
) -> f64 {
    let mut sum_ft = 0.0f64;
    let mut sum_ff = 0.0f64;
    let mut sum_tt = 0.0f64;
    let mut count = 0u32;

    let sample_step = 3; // Sample every 3rd pixel for speed

    for ty in (0..t_h).step_by(sample_step) {
        for tx in (0..t_w).step_by(sample_step) {
            let fx = offset_x + tx;
            let fy = offset_y + ty;

            let f_idx = ((fy * frame_w + fx) * 4) as usize;
            let t_idx = ((ty * t_w + tx) * 4) as usize;

            if f_idx + 2 >= frame.len() || t_idx + 2 >= template.len() {
                continue;
            }

            // Grayscale comparison
            let f_gray = (frame[f_idx] as f64 * 0.299
                + frame[f_idx + 1] as f64 * 0.587
                + frame[f_idx + 2] as f64 * 0.114)
                / 255.0;

            let t_gray = (template[t_idx] as f64 * 0.299
                + template[t_idx + 1] as f64 * 0.587
                + template[t_idx + 2] as f64 * 0.114)
                / 255.0;

            sum_ft += f_gray * t_gray;
            sum_ff += f_gray * f_gray;
            sum_tt += t_gray * t_gray;
            count += 1;
        }
    }

    if count == 0 || sum_ff < f64::EPSILON || sum_tt < f64::EPSILON {
        return 0.0;
    }

    sum_ft / (sum_ff.sqrt() * sum_tt.sqrt())
}

/// Load a template image from PNG file.
pub fn load_template(path: &str) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::open(path).map_err(|e| format!("Failed to load template: {}", e))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((rgba.into_raw(), w, h))
}
