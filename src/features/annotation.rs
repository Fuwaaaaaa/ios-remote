use serde::{Deserialize, Serialize};

/// Annotation layer: draw arrows, text, rectangles on top of the mirrored screen.
///
/// Annotations are stored as a list and composited onto the frame before display.
/// Useful for bug reports, presentations, and documentation.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnnotationLayer {
    pub items: Vec<Annotation>,
    pub visible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Annotation {
    /// Rectangle outline
    Rect { x: u32, y: u32, w: u32, h: u32, color: [u8; 3], thickness: u8 },
    /// Arrow from (x1,y1) to (x2,y2)
    Arrow { x1: u32, y1: u32, x2: u32, y2: u32, color: [u8; 3] },
    /// Text label at position
    Text { x: u32, y: u32, text: String, color: [u8; 3] },
    /// Freehand drawing path
    Freehand { points: Vec<(u32, u32)>, color: [u8; 3], thickness: u8 },
    /// Highlight (semi-transparent filled rect)
    Highlight { x: u32, y: u32, w: u32, h: u32, color: [u8; 3], alpha: f32 },
}

impl AnnotationLayer {
    pub fn new() -> Self {
        Self { items: Vec::new(), visible: true }
    }

    pub fn add(&mut self, annotation: Annotation) {
        self.items.push(annotation);
    }

    pub fn undo(&mut self) {
        self.items.pop();
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Draw all annotations onto an RGBA buffer.
    pub fn render(&self, rgba: &mut [u8], width: u32, height: u32) {
        if !self.visible {
            return;
        }

        for item in &self.items {
            match item {
                Annotation::Rect { x, y, w, h, color, thickness } => {
                    draw_rect(rgba, width, height, *x, *y, *w, *h, *color, *thickness);
                }
                Annotation::Arrow { x1, y1, x2, y2, color } => {
                    draw_arrow(rgba, width, height, *x1, *y1, *x2, *y2, *color);
                }
                Annotation::Text { x, y, text, color } => {
                    draw_text(rgba, width, *x, *y, text, *color);
                }
                Annotation::Freehand { points, color, thickness } => {
                    for window in points.windows(2) {
                        draw_thick_line(rgba, width, height, window[0], window[1], *color, *thickness);
                    }
                }
                Annotation::Highlight { x, y, w, h, color, alpha } => {
                    draw_highlight(rgba, width, height, *x, *y, *w, *h, *color, *alpha);
                }
            }
        }
    }
}

fn set_pixel(rgba: &mut [u8], width: u32, x: u32, y: u32, color: [u8; 3]) {
    let idx = ((y * width + x) * 4) as usize;
    if idx + 2 < rgba.len() {
        rgba[idx] = color[0];
        rgba[idx + 1] = color[1];
        rgba[idx + 2] = color[2];
    }
}

fn draw_rect(rgba: &mut [u8], w: u32, h: u32, x: u32, y: u32, rw: u32, rh: u32, color: [u8; 3], t: u8) {
    let t = t as u32;
    for i in 0..t {
        for px in x.saturating_sub(i)..=(x + rw + i).min(w - 1) {
            if y >= i { set_pixel(rgba, w, px, y - i, color); }
            if y + rh + i < h { set_pixel(rgba, w, px, y + rh + i, color); }
        }
        for py in y.saturating_sub(i)..=(y + rh + i).min(h - 1) {
            if x >= i { set_pixel(rgba, w, x - i, py, color); }
            if x + rw + i < w { set_pixel(rgba, w, x + rw + i, py, color); }
        }
    }
}

fn draw_arrow(rgba: &mut [u8], w: u32, h: u32, x1: u32, y1: u32, x2: u32, y2: u32, color: [u8; 3]) {
    draw_thick_line(rgba, w, h, (x1, y1), (x2, y2), color, 2);
    // Arrowhead
    let dx = x2 as f32 - x1 as f32;
    let dy = y2 as f32 - y1 as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 { return; }
    let ux = dx / len;
    let uy = dy / len;
    let head_len = 12.0f32;
    let head_width = 6.0f32;
    let bx = x2 as f32 - ux * head_len;
    let by = y2 as f32 - uy * head_len;
    let lx = (bx - uy * head_width) as u32;
    let ly = (by + ux * head_width) as u32;
    let rx = (bx + uy * head_width) as u32;
    let ry = (by - ux * head_width) as u32;
    draw_thick_line(rgba, w, h, (x2, y2), (lx, ly), color, 2);
    draw_thick_line(rgba, w, h, (x2, y2), (rx, ry), color, 2);
}

fn draw_thick_line(rgba: &mut [u8], w: u32, h: u32, from: (u32, u32), to: (u32, u32), color: [u8; 3], thickness: u8) {
    let t = thickness as i32;
    let (mut x0, mut y0) = (from.0 as i32, from.1 as i32);
    let (x1, y1) = (to.0 as i32, to.1 as i32);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        for ty in -t/2..=t/2 {
            for tx in -t/2..=t/2 {
                let px = x0 + tx;
                let py = y0 + ty;
                if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                    set_pixel(rgba, w, px as u32, py as u32, color);
                }
            }
        }
        if x0 == x1 && y0 == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x0 += sx; }
        if e2 <= dx { err += dx; y0 += sy; }
    }
}

fn draw_highlight(rgba: &mut [u8], w: u32, h: u32, x: u32, y: u32, rw: u32, rh: u32, color: [u8; 3], alpha: f32) {
    for py in y..(y + rh).min(h) {
        for px in x..(x + rw).min(w) {
            let idx = ((py * w + px) * 4) as usize;
            if idx + 2 < rgba.len() {
                rgba[idx] = ((rgba[idx] as f32) * (1.0 - alpha) + color[0] as f32 * alpha) as u8;
                rgba[idx + 1] = ((rgba[idx + 1] as f32) * (1.0 - alpha) + color[1] as f32 * alpha) as u8;
                rgba[idx + 2] = ((rgba[idx + 2] as f32) * (1.0 - alpha) + color[2] as f32 * alpha) as u8;
            }
        }
    }
}

fn draw_text(rgba: &mut [u8], width: u32, x: u32, y: u32, text: &str, color: [u8; 3]) {
    // Reuse bitmap font from stats_overlay concept — simple 5x7 chars
    for (i, _ch) in text.chars().enumerate() {
        let px = x + (i as u32) * 6;
        if px + 5 >= width { break; }
        // Simple filled block per character as placeholder
        for dy in 0..7u32 {
            for dx in 0..5u32 {
                set_pixel(rgba, width, px + dx, y + dy, color);
            }
        }
    }
}
