/// Touch heatmap: visualize where taps/clicks happen most frequently.

pub struct Heatmap {
    grid: Vec<Vec<u32>>,
    grid_w: usize,
    grid_h: usize,
    cell_size: u32,
}

impl Heatmap {
    pub fn new(screen_w: u32, screen_h: u32, cell_size: u32) -> Self {
        let gw = (screen_w / cell_size + 1) as usize;
        let gh = (screen_h / cell_size + 1) as usize;
        Self {
            grid: vec![vec![0u32; gw]; gh],
            grid_w: gw, grid_h: gh, cell_size,
        }
    }

    pub fn record_tap(&mut self, x: u32, y: u32) {
        let gx = (x / self.cell_size) as usize;
        let gy = (y / self.cell_size) as usize;
        if gy < self.grid_h && gx < self.grid_w {
            self.grid[gy][gx] += 1;
        }
    }

    pub fn clear(&mut self) {
        for row in &mut self.grid { for cell in row.iter_mut() { *cell = 0; } }
    }

    fn max_value(&self) -> u32 {
        self.grid.iter().flat_map(|r| r.iter()).copied().max().unwrap_or(1).max(1)
    }

    /// Render heatmap overlay onto RGBA buffer.
    pub fn draw(&self, rgba: &mut [u8], buf_w: u32, buf_h: u32, alpha: f32) {
        let max_val = self.max_value() as f32;
        for gy in 0..self.grid_h {
            for gx in 0..self.grid_w {
                let val = self.grid[gy][gx];
                if val == 0 { continue; }
                let intensity = val as f32 / max_val;
                let (cr, cg, cb) = heat_color(intensity);
                let px_start = gx as u32 * self.cell_size;
                let py_start = gy as u32 * self.cell_size;
                for dy in 0..self.cell_size {
                    for dx in 0..self.cell_size {
                        let px = px_start + dx;
                        let py = py_start + dy;
                        if px >= buf_w || py >= buf_h { continue; }
                        let idx = ((py * buf_w + px) * 4) as usize;
                        if idx + 2 < rgba.len() {
                            let a = alpha * intensity;
                            rgba[idx] = blend(rgba[idx], cr, a);
                            rgba[idx + 1] = blend(rgba[idx + 1], cg, a);
                            rgba[idx + 2] = blend(rgba[idx + 2], cb, a);
                        }
                    }
                }
            }
        }
    }
}

/// Blue → Green → Yellow → Red gradient.
fn heat_color(t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    if t < 0.33 {
        let f = t / 0.33;
        (0, (f * 255.0) as u8, ((1.0 - f) * 255.0) as u8)
    } else if t < 0.66 {
        let f = (t - 0.33) / 0.33;
        ((f * 255.0) as u8, 255, 0)
    } else {
        let f = (t - 0.66) / 0.34;
        (255, ((1.0 - f) * 255.0) as u8, 0)
    }
}

fn blend(bg: u8, fg: u8, alpha: f32) -> u8 {
    ((bg as f32 * (1.0 - alpha)) + (fg as f32 * alpha)).clamp(0.0, 255.0) as u8
}
