/// Zoom & pan: magnify a region of the mirrored screen.
///
/// The user can scroll to zoom in/out and drag to pan.
/// The zoomed view is applied before display rendering.

#[derive(Clone, Debug)]
pub struct ZoomState {
    /// Zoom level: 1.0 = normal, 2.0 = 2x zoom, etc.
    pub level: f32,
    /// Pan offset in source pixels from top-left.
    pub offset_x: f32,
    pub offset_y: f32,
    /// Source frame dimensions.
    pub src_width: u32,
    pub src_height: u32,
}

impl ZoomState {
    pub fn new() -> Self {
        Self {
            level: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            src_width: 1920,
            src_height: 1080,
        }
    }

    /// Zoom in/out by delta. Positive = zoom in.
    pub fn zoom(&mut self, delta: f32, mouse_x: f32, mouse_y: f32) {
        let old_level = self.level;
        self.level = (self.level + delta * 0.1).clamp(1.0, 10.0);

        // Zoom toward mouse position
        if self.level != old_level {
            let scale = self.level / old_level;
            self.offset_x = mouse_x - (mouse_x - self.offset_x) * scale;
            self.offset_y = mouse_y - (mouse_y - self.offset_y) * scale;
            self.clamp_offset();
        }
    }

    /// Pan by delta pixels.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.offset_x += dx / self.level;
        self.offset_y += dy / self.level;
        self.clamp_offset();
    }

    /// Reset to 1:1 view.
    pub fn reset(&mut self) {
        self.level = 1.0;
        self.offset_x = 0.0;
        self.offset_y = 0.0;
    }

    fn clamp_offset(&mut self) {
        let max_x = self.src_width as f32 * (1.0 - 1.0 / self.level);
        let max_y = self.src_height as f32 * (1.0 - 1.0 / self.level);
        self.offset_x = self.offset_x.clamp(0.0, max_x.max(0.0));
        self.offset_y = self.offset_y.clamp(0.0, max_y.max(0.0));
    }

    /// Apply zoom to extract a sub-region from the source RGBA buffer.
    /// Returns (cropped_rgba, crop_width, crop_height).
    pub fn apply(&self, rgba: &[u8], src_w: u32, src_h: u32) -> (Vec<u8>, u32, u32) {
        if self.level <= 1.01 {
            return (rgba.to_vec(), src_w, src_h);
        }

        let view_w = (src_w as f32 / self.level) as u32;
        let view_h = (src_h as f32 / self.level) as u32;
        let ox = self.offset_x as u32;
        let oy = self.offset_y as u32;

        let mut out = vec![0u8; (view_w * view_h * 4) as usize];

        for y in 0..view_h {
            for x in 0..view_w {
                let sx = (ox + x).min(src_w - 1);
                let sy = (oy + y).min(src_h - 1);
                let src_idx = ((sy * src_w + sx) * 4) as usize;
                let dst_idx = ((y * view_w + x) * 4) as usize;

                if src_idx + 3 < rgba.len() && dst_idx + 3 < out.len() {
                    out[dst_idx..dst_idx + 4].copy_from_slice(&rgba[src_idx..src_idx + 4]);
                }
            }
        }

        (out, view_w, view_h)
    }
}
