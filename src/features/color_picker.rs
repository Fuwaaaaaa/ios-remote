use super::Frame;

/// Color picker: get the color of a pixel at the mouse position.
///
/// Returns color in multiple formats: HEX, RGB, HSL.

#[derive(Debug, Clone)]
pub struct PickedColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
    pub hex: String,
    pub rgb: String,
    pub hsl: String,
}

/// Pick the color at (x, y) from the frame.
pub fn pick_color(frame: &Frame, x: u32, y: u32) -> Option<PickedColor> {
    if x >= frame.width || y >= frame.height {
        return None;
    }

    let idx = ((y * frame.width + x) * 4) as usize;
    if idx + 3 >= frame.rgba.len() {
        return None;
    }

    let r = frame.rgba[idx];
    let g = frame.rgba[idx + 1];
    let b = frame.rgba[idx + 2];
    let a = frame.rgba[idx + 3];

    let (h, s, l) = rgb_to_hsl(r, g, b);

    Some(PickedColor {
        r,
        g,
        b,
        a,
        hex: format!("#{:02X}{:02X}{:02X}", r, g, b),
        rgb: format!("rgb({}, {}, {})", r, g, b),
        hsl: format!("hsl({:.0}, {:.0}%, {:.0}%)", h, s * 100.0, l * 100.0),
    })
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - r).abs() < f32::EPSILON {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
    } else if (max - g).abs() < f32::EPSILON {
        ((b - r) / d + 2.0) * 60.0
    } else {
        ((r - g) / d + 4.0) * 60.0
    };

    (h, s, l)
}

/// Draw color picker crosshair + info at mouse position.
pub fn draw_picker_overlay(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    color: &PickedColor,
) {
    let w = width;

    // Crosshair lines (10px each direction)
    for d in 1..=10u32 {
        for &(px, py) in &[
            (x.saturating_sub(d), y),
            (x + d, y),
            (x, y.saturating_sub(d)),
            (x, y + d),
        ] {
            if px < w && py < height {
                let idx = ((py * w + px) * 4) as usize;
                if idx + 2 < rgba.len() {
                    rgba[idx] = 255;
                    rgba[idx + 1] = 255;
                    rgba[idx + 2] = 255;
                }
            }
        }
    }

    // Color preview box (16x16) next to cursor
    let bx = (x + 15).min(w - 20);
    let by = (y + 15).min(height - 20);
    for dy in 0..16u32 {
        for dx in 0..16u32 {
            let px = bx + dx;
            let py = by + dy;
            if px < w && py < height {
                let idx = ((py * w + px) * 4) as usize;
                if idx + 2 < rgba.len() {
                    rgba[idx] = color.r;
                    rgba[idx + 1] = color.g;
                    rgba[idx + 2] = color.b;
                }
            }
        }
    }
}
