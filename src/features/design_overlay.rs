//! Design overlays: safe area, grid, and accessibility tools for UI development.

/// iOS safe area inset overlay.
pub struct SafeAreaOverlay {
    pub top: u32,    // Dynamic Island / notch
    pub bottom: u32, // Home indicator
    pub left: u32,
    pub right: u32,
    pub visible: bool,
}

impl SafeAreaOverlay {
    /// iPhone 15 Pro safe area insets (logical points → pixels at 3x).
    pub fn iphone_15_pro() -> Self {
        Self { top: 59 * 3, bottom: 34 * 3, left: 0, right: 0, visible: true }
    }

    pub fn draw(&self, rgba: &mut [u8], w: u32, h: u32) {
        if !self.visible { return; }
        let color = [255u8, 0, 0]; // red tint
        let alpha = 0.15f32;

        // Top unsafe area
        tint_region(rgba, w, 0, 0, w, self.top.min(h), color, alpha);
        // Bottom unsafe area
        let bot_y = h.saturating_sub(self.bottom);
        tint_region(rgba, w, 0, bot_y, w, self.bottom.min(h), color, alpha);
        // Left
        if self.left > 0 { tint_region(rgba, w, 0, self.top, self.left, h - self.top - self.bottom, color, alpha); }
        // Right
        if self.right > 0 { tint_region(rgba, w, w - self.right, self.top, self.right, h - self.top - self.bottom, color, alpha); }

        // Draw border lines
        draw_h_line(rgba, w, 0, self.top, w, [255, 0, 0]);
        draw_h_line(rgba, w, 0, bot_y, w, [255, 0, 0]);
    }
}

/// Design grid overlay (8pt or 4pt grid).
pub struct GridOverlay {
    pub spacing: u32,
    pub color: [u8; 3],
    pub visible: bool,
}

impl GridOverlay {
    pub fn new_8pt() -> Self { Self { spacing: 8 * 3, color: [0, 150, 255], visible: false } }
    pub fn new_4pt() -> Self { Self { spacing: 4 * 3, color: [100, 100, 255], visible: false } }

    pub fn draw(&self, rgba: &mut [u8], w: u32, h: u32) {
        if !self.visible { return; }
        for y in (0..h).step_by(self.spacing as usize) {
            draw_h_line(rgba, w, 0, y, w, self.color);
        }
        for x in (0..w).step_by(self.spacing as usize) {
            draw_v_line(rgba, w, h, x, 0, h, self.color);
        }
    }
}

/// WCAG contrast ratio checker.
pub fn contrast_ratio(fg: [u8; 3], bg: [u8; 3]) -> f64 {
    let l1 = relative_luminance(fg);
    let l2 = relative_luminance(bg);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Check if contrast meets WCAG AA (4.5:1 for normal text, 3:1 for large).
pub fn wcag_aa_pass(ratio: f64, large_text: bool) -> bool {
    if large_text { ratio >= 3.0 } else { ratio >= 4.5 }
}

fn relative_luminance(c: [u8; 3]) -> f64 {
    let srgb = |v: u8| -> f64 {
        let s = v as f64 / 255.0;
        if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * srgb(c[0]) + 0.7152 * srgb(c[1]) + 0.0722 * srgb(c[2])
}

/// Color blindness simulation filters.
pub fn simulate_color_blindness(rgba: &mut [u8], mode: ColorBlindMode) {
    let len = rgba.len() / 4;
    for i in 0..len {
        let idx = i * 4;
        if idx + 2 >= rgba.len() { break; }
        let (r, g, b) = (rgba[idx] as f32, rgba[idx + 1] as f32, rgba[idx + 2] as f32);
        let (nr, ng, nb) = match mode {
            ColorBlindMode::Protanopia => (0.567*r + 0.433*g, 0.558*r + 0.442*g, 0.242*g + 0.758*b),
            ColorBlindMode::Deuteranopia => (0.625*r + 0.375*g, 0.7*r + 0.3*g, 0.3*g + 0.7*b),
            ColorBlindMode::Tritanopia => (0.95*r + 0.05*g, 0.433*g + 0.567*b, 0.475*g + 0.525*b),
            ColorBlindMode::Achromatopsia => { let gray = 0.299*r + 0.587*g + 0.114*b; (gray, gray, gray) }
        };
        rgba[idx] = nr.clamp(0.0, 255.0) as u8;
        rgba[idx + 1] = ng.clamp(0.0, 255.0) as u8;
        rgba[idx + 2] = nb.clamp(0.0, 255.0) as u8;
    }
}

#[derive(Clone, Debug)]
pub enum ColorBlindMode { Protanopia, Deuteranopia, Tritanopia, Achromatopsia }

fn tint_region(rgba: &mut [u8], w: u32, x: u32, y: u32, rw: u32, rh: u32, color: [u8; 3], alpha: f32) {
    for py in y..(y + rh) { for px in x..(x + rw).min(w) {
        let idx = ((py * w + px) * 4) as usize;
        if idx + 2 < rgba.len() {
            rgba[idx] = ((rgba[idx] as f32) * (1.0 - alpha) + color[0] as f32 * alpha) as u8;
            rgba[idx + 1] = ((rgba[idx + 1] as f32) * (1.0 - alpha) + color[1] as f32 * alpha) as u8;
            rgba[idx + 2] = ((rgba[idx + 2] as f32) * (1.0 - alpha) + color[2] as f32 * alpha) as u8;
        }
    }}
}

fn draw_h_line(rgba: &mut [u8], w: u32, x: u32, y: u32, len: u32, color: [u8; 3]) {
    for px in x..(x + len).min(w) {
        let idx = ((y * w + px) * 4) as usize;
        if idx + 2 < rgba.len() { rgba[idx] = color[0]; rgba[idx + 1] = color[1]; rgba[idx + 2] = color[2]; }
    }
}

fn draw_v_line(rgba: &mut [u8], w: u32, h: u32, x: u32, y: u32, len: u32, color: [u8; 3]) {
    for py in y..(y + len).min(h) {
        let idx = ((py * w + x) * 4) as usize;
        if idx + 2 < rgba.len() { rgba[idx] = color[0]; rgba[idx + 1] = color[1]; rgba[idx + 2] = color[2]; }
    }
}
