
/// Privacy mode: blur/redact sensitive areas of the screen.
///
/// Useful when recording or streaming to hide passwords, personal info,
/// or notification content. Areas can be manually defined or auto-detected.

#[derive(Clone, Debug)]
pub struct PrivacyZone {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub mode: BlurMode,
}

#[derive(Clone, Debug)]
pub enum BlurMode {
    /// Gaussian-like box blur
    Blur { radius: u32 },
    /// Solid color fill
    Solid { color: [u8; 3] },
    /// Pixelate (mosaic)
    Pixelate { block_size: u32 },
}

/// Apply privacy zones to an RGBA frame buffer (in-place).
pub fn apply_privacy_zones(rgba: &mut [u8], width: u32, height: u32, zones: &[PrivacyZone]) {
    for zone in zones {
        match zone.mode {
            BlurMode::Blur { radius } => {
                box_blur_region(rgba, width, height, zone.x, zone.y, zone.w, zone.h, radius);
            }
            BlurMode::Solid { color } => {
                fill_region(rgba, width, zone.x, zone.y, zone.w, zone.h, color);
            }
            BlurMode::Pixelate { block_size } => {
                pixelate_region(rgba, width, height, zone.x, zone.y, zone.w, zone.h, block_size);
            }
        }
    }
}

fn box_blur_region(rgba: &mut [u8], w: u32, h: u32, x: u32, y: u32, rw: u32, rh: u32, radius: u32) {
    let r = radius as i32;

    // Simple multi-pass box blur (2 passes ≈ decent gaussian approximation)
    for _pass in 0..2 {
        for py in y..(y + rh).min(h) {
            for px in x..(x + rw).min(w) {
                let mut sum_r = 0u32;
                let mut sum_g = 0u32;
                let mut sum_b = 0u32;
                let mut count = 0u32;

                for dy in -r..=r {
                    for dx in -r..=r {
                        let sx = px as i32 + dx;
                        let sy = py as i32 + dy;
                        if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                            let idx = ((sy as u32 * w + sx as u32) * 4) as usize;
                            if idx + 2 < rgba.len() {
                                sum_r += rgba[idx] as u32;
                                sum_g += rgba[idx + 1] as u32;
                                sum_b += rgba[idx + 2] as u32;
                                count += 1;
                            }
                        }
                    }
                }

                if count > 0 {
                    let idx = ((py * w + px) * 4) as usize;
                    if idx + 2 < rgba.len() {
                        rgba[idx] = (sum_r / count) as u8;
                        rgba[idx + 1] = (sum_g / count) as u8;
                        rgba[idx + 2] = (sum_b / count) as u8;
                    }
                }
            }
        }
    }
}

fn fill_region(rgba: &mut [u8], w: u32, x: u32, y: u32, rw: u32, rh: u32, color: [u8; 3]) {
    for py in y..(y + rh) {
        for px in x..(x + rw) {
            let idx = ((py * w + px) * 4) as usize;
            if idx + 2 < rgba.len() {
                rgba[idx] = color[0];
                rgba[idx + 1] = color[1];
                rgba[idx + 2] = color[2];
            }
        }
    }
}

fn pixelate_region(rgba: &mut [u8], w: u32, h: u32, x: u32, y: u32, rw: u32, rh: u32, block: u32) {
    let block = block.max(2);
    let mut by = y;
    while by < (y + rh).min(h) {
        let mut bx = x;
        while bx < (x + rw).min(w) {
            // Average color in this block
            let mut sum_r = 0u32;
            let mut sum_g = 0u32;
            let mut sum_b = 0u32;
            let mut count = 0u32;

            for dy in 0..block {
                for dx in 0..block {
                    let px = bx + dx;
                    let py = by + dy;
                    if px < w && py < h {
                        let idx = ((py * w + px) * 4) as usize;
                        if idx + 2 < rgba.len() {
                            sum_r += rgba[idx] as u32;
                            sum_g += rgba[idx + 1] as u32;
                            sum_b += rgba[idx + 2] as u32;
                            count += 1;
                        }
                    }
                }
            }

            if count > 0 {
                let avg = [(sum_r / count) as u8, (sum_g / count) as u8, (sum_b / count) as u8];
                for dy in 0..block {
                    for dx in 0..block {
                        let px = bx + dx;
                        let py = by + dy;
                        if px < w && py < h {
                            let idx = ((py * w + px) * 4) as usize;
                            if idx + 2 < rgba.len() {
                                rgba[idx] = avg[0];
                                rgba[idx + 1] = avg[1];
                                rgba[idx + 2] = avg[2];
                            }
                        }
                    }
                }
            }

            bx += block;
        }
        by += block;
    }
}
