//! Presentation mode: fullscreen display with laser pointer.

pub struct PresentationMode {
    pub enabled: bool,
    pub laser_x: u32,
    pub laser_y: u32,
    pub laser_visible: bool,
    pub cursor_hidden: bool,
}

impl PresentationMode {
    pub fn new() -> Self {
        Self { enabled: false, laser_x: 0, laser_y: 0, laser_visible: false, cursor_hidden: false }
    }

    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.cursor_hidden = self.enabled;
        self.enabled
    }

    pub fn move_laser(&mut self, x: u32, y: u32) {
        self.laser_x = x;
        self.laser_y = y;
        self.laser_visible = true;
    }

    /// Draw laser pointer dot onto RGBA buffer.
    pub fn draw_laser(&self, rgba: &mut [u8], w: u32, h: u32) {
        if !self.laser_visible || !self.enabled { return; }
        let radius = 8u32;
        let cx = self.laser_x;
        let cy = self.laser_y;

        for dy in 0..radius * 2 {
            for dx in 0..radius * 2 {
                let px = cx + dx - radius;
                let py = cy + dy - radius;
                if px >= w || py >= h { continue; }
                let dist = (((dx as i32 - radius as i32).pow(2) + (dy as i32 - radius as i32).pow(2)) as f32).sqrt();
                if dist <= radius as f32 {
                    let alpha = 1.0 - (dist / radius as f32);
                    let idx = ((py * w + px) * 4) as usize;
                    if idx + 2 < rgba.len() {
                        rgba[idx] = blend(rgba[idx], 255, alpha);
                        rgba[idx + 1] = blend(rgba[idx + 1], 0, alpha * 0.8);
                        rgba[idx + 2] = blend(rgba[idx + 2], 0, alpha * 0.8);
                    }
                }
            }
        }
    }
}

fn blend(bg: u8, fg: u8, a: f32) -> u8 {
    ((bg as f32 * (1.0 - a)) + (fg as f32 * a)).clamp(0.0, 255.0) as u8
}
