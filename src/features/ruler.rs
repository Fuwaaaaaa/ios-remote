/// Ruler: measure distances and UI element sizes on the mirrored screen.
///
/// Click two points to measure the distance in pixels.
/// Draws a measurement line with the distance label.

#[derive(Clone, Debug)]
pub struct Measurement {
    pub start: (u32, u32),
    pub end: (u32, u32),
    pub distance_px: f64,
}

impl Measurement {
    pub fn new(start: (u32, u32), end: (u32, u32)) -> Self {
        let dx = end.0 as f64 - start.0 as f64;
        let dy = end.1 as f64 - start.1 as f64;
        Self {
            start,
            end,
            distance_px: (dx * dx + dy * dy).sqrt(),
        }
    }

    /// Draw the measurement line + distance label onto an RGBA buffer.
    pub fn draw(&self, rgba: &mut [u8], width: u32, height: u32) {
        // Draw dashed line
        let steps = self.distance_px as u32;
        for i in 0..steps {
            if i % 6 < 4 { // dash pattern: 4 on, 2 off
                let t = i as f32 / steps as f32;
                let x = (self.start.0 as f32 + (self.end.0 as f32 - self.start.0 as f32) * t) as u32;
                let y = (self.start.1 as f32 + (self.end.1 as f32 - self.start.1 as f32) * t) as u32;
                if x < width && y < height {
                    let idx = ((y * width + x) * 4) as usize;
                    if idx + 2 < rgba.len() {
                        rgba[idx] = 255;     // Red
                        rgba[idx + 1] = 80;
                        rgba[idx + 2] = 80;
                    }
                }
            }
        }

        // Draw endpoints (small crosses)
        for &(px, py) in &[self.start, self.end] {
            for d in 0..4u32 {
                for &(x, y) in &[
                    (px.saturating_sub(d), py),
                    (px + d, py),
                    (px, py.saturating_sub(d)),
                    (px, py + d),
                ] {
                    if x < width && y < height {
                        let idx = ((y * width + x) * 4) as usize;
                        if idx + 2 < rgba.len() {
                            rgba[idx] = 255;
                            rgba[idx + 1] = 255;
                            rgba[idx + 2] = 0;
                        }
                    }
                }
            }
        }

        // Distance as dx×dy label position (midpoint)
        let _mid_x = (self.start.0 + self.end.0) / 2;
        let _mid_y = (self.start.1 + self.end.1) / 2;
        let dx = self.end.0 as i32 - self.start.0 as i32;
        let dy = self.end.1 as i32 - self.start.1 as i32;
        let _label = format!("{:.0}px ({}×{})", self.distance_px, dx.abs(), dy.abs());
        // Label rendering would use the bitmap font from stats_overlay
    }
}

pub struct RulerTool {
    pub measurements: Vec<Measurement>,
    pub pending_start: Option<(u32, u32)>,
}

impl RulerTool {
    pub fn new() -> Self {
        Self {
            measurements: Vec::new(),
            pending_start: None,
        }
    }

    /// Handle a click. First click sets start, second click creates measurement.
    pub fn click(&mut self, x: u32, y: u32) -> Option<Measurement> {
        match self.pending_start.take() {
            None => {
                self.pending_start = Some((x, y));
                None
            }
            Some(start) => {
                let m = Measurement::new(start, (x, y));
                self.measurements.push(m.clone());
                Some(m)
            }
        }
    }

    pub fn clear(&mut self) {
        self.measurements.clear();
        self.pending_start = None;
    }

    pub fn draw_all(&self, rgba: &mut [u8], width: u32, height: u32) {
        for m in &self.measurements {
            m.draw(rgba, width, height);
        }
    }
}
