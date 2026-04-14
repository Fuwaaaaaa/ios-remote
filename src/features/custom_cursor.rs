/// Custom mouse cursor overlay on the mirrored screen.

#[derive(Clone, Debug)]
pub enum CursorStyle {
    Crosshair,
    Circle { radius: u32 },
    Arrow,
    Dot,
    Hidden,
}

pub struct CursorOverlay {
    pub style: CursorStyle,
    pub color: [u8; 3],
    pub x: u32,
    pub y: u32,
    pub visible: bool,
}

impl CursorOverlay {
    pub fn new() -> Self {
        Self { style: CursorStyle::Crosshair, color: [255, 255, 255], x: 0, y: 0, visible: true }
    }

    pub fn update_pos(&mut self, x: u32, y: u32) { self.x = x; self.y = y; }

    pub fn draw(&self, rgba: &mut [u8], w: u32, h: u32) {
        if !self.visible { return; }
        match self.style {
            CursorStyle::Crosshair => self.draw_crosshair(rgba, w, h),
            CursorStyle::Circle { radius } => self.draw_circle(rgba, w, h, radius),
            CursorStyle::Dot => self.draw_circle(rgba, w, h, 3),
            CursorStyle::Arrow => self.draw_arrow(rgba, w, h),
            CursorStyle::Hidden => {}
        }
    }

    fn draw_crosshair(&self, rgba: &mut [u8], w: u32, h: u32) {
        let len = 15u32;
        let gap = 3u32;
        for d in gap..len {
            for &(px, py) in &[
                (self.x.saturating_sub(d), self.y),
                (self.x + d, self.y),
                (self.x, self.y.saturating_sub(d)),
                (self.x, self.y + d),
            ] {
                self.set_px(rgba, w, h, px, py);
            }
        }
    }

    fn draw_circle(&self, rgba: &mut [u8], w: u32, h: u32, r: u32) {
        let r2 = (r * r) as i64;
        for dy in 0..=r * 2 {
            for dx in 0..=r * 2 {
                let ddx = dx as i64 - r as i64;
                let ddy = dy as i64 - r as i64;
                let d2 = ddx * ddx + ddy * ddy;
                if d2 <= r2 && d2 >= (r.saturating_sub(1) as i64).pow(2) {
                    let px = self.x + dx - r;
                    let py = self.y + dy - r;
                    self.set_px(rgba, w, h, px, py);
                }
            }
        }
    }

    fn draw_arrow(&self, rgba: &mut [u8], w: u32, h: u32) {
        // Simple arrow pointing down-right
        for d in 0..12u32 {
            self.set_px(rgba, w, h, self.x + d, self.y + d);
            if d < 6 { self.set_px(rgba, w, h, self.x, self.y + d); }
            if d < 6 { self.set_px(rgba, w, h, self.x + d, self.y); }
        }
    }

    fn set_px(&self, rgba: &mut [u8], w: u32, h: u32, x: u32, y: u32) {
        if x >= w || y >= h { return; }
        let idx = ((y * w + x) * 4) as usize;
        if idx + 2 < rgba.len() {
            rgba[idx] = self.color[0];
            rgba[idx + 1] = self.color[1];
            rgba[idx + 2] = self.color[2];
        }
    }
}

/// Window snap: snap display window to screen edges.
pub fn snap_position(window_x: i32, window_y: i32, window_w: u32, window_h: u32, screen_w: u32, screen_h: u32, margin: i32) -> (i32, i32) {
    let mut x = window_x;
    let mut y = window_y;
    if x.abs() < margin { x = 0; }
    if y.abs() < margin { y = 0; }
    if ((x + window_w as i32) - screen_w as i32).abs() < margin { x = screen_w as i32 - window_w as i32; }
    if ((y + window_h as i32) - screen_h as i32).abs() < margin { y = screen_h as i32 - window_h as i32; }
    (x, y)
}
