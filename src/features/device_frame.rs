//! Device frame: render iPhone hardware frame around the mirrored screen.
//!
//! Draws a stylized iPhone bezel for screenshots and recordings.

pub struct DeviceFrame {
    pub model: DeviceModel,
    pub show_frame: bool,
}

#[derive(Clone, Debug)]
pub enum DeviceModel {
    IPhone15Pro,
    IPhone15,
    IPhoneSE,
    IPadPro,
    Custom {
        corner_radius: u32,
        bezel_width: u32,
        notch: bool,
    },
}

impl DeviceFrame {
    pub fn new(model: DeviceModel) -> Self {
        Self {
            model,
            show_frame: true,
        }
    }

    /// Render frame with device bezel. Returns new RGBA with bezel.
    pub fn apply(&self, rgba: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
        if !self.show_frame {
            return (rgba.to_vec(), w, h);
        }
        let (bezel, corner, notch) = match &self.model {
            DeviceModel::IPhone15Pro => (20, 50, true),
            DeviceModel::IPhone15 => (18, 45, true),
            DeviceModel::IPhoneSE => (24, 20, false),
            DeviceModel::IPadPro => (16, 30, false),
            DeviceModel::Custom {
                corner_radius,
                bezel_width,
                notch,
            } => (*bezel_width, *corner_radius, *notch),
        };

        let out_w = w + bezel * 2;
        let out_h = h + bezel * 2;
        let mut out = vec![0x22u8; (out_w * out_h * 4) as usize];
        for i in 0..(out_w * out_h) as usize {
            out[i * 4 + 3] = 255;
        }

        // Bezel color: dark gray with metallic look
        for y in 0..out_h {
            for x in 0..out_w {
                let idx = ((y * out_w + x) * 4) as usize;
                if idx + 2 >= out.len() {
                    continue;
                }
                out[idx] = 0x33;
                out[idx + 1] = 0x33;
                out[idx + 2] = 0x38;
            }
        }

        // Copy screen content into bezel
        for y in 0..h {
            for x in 0..w {
                let src = ((y * w + x) * 4) as usize;
                let dst = (((y + bezel) * out_w + (x + bezel)) * 4) as usize;
                if src + 3 < rgba.len() && dst + 3 < out.len() {
                    out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
                }
            }
        }

        // Dynamic Island / notch indicator
        if notch {
            let cx = out_w / 2;
            let ny = bezel / 2;
            let nw = 80u32;
            let nh = 20u32;
            for dy in 0..nh {
                for dx in 0..nw {
                    let px = cx - nw / 2 + dx;
                    let py = ny - nh / 2 + dy;
                    let idx = ((py * out_w + px) * 4) as usize;
                    if idx + 2 < out.len() {
                        out[idx] = 0x11;
                        out[idx + 1] = 0x11;
                        out[idx + 2] = 0x11;
                    }
                }
            }
        }

        // Round corners (clear to transparent)
        round_corners(&mut out, out_w, out_h, corner);

        (out, out_w, out_h)
    }
}

fn round_corners(rgba: &mut [u8], w: u32, h: u32, r: u32) {
    let r2 = (r * r) as i64;
    for corner in &[(0u32, 0u32), (w - r, 0), (0, h - r), (w - r, h - r)] {
        for dy in 0..r {
            for dx in 0..r {
                let cx = if corner.0 == 0 { r - dx } else { dx };
                let cy = if corner.1 == 0 { r - dy } else { dy };
                let dist = cx as i64 * cx as i64 + cy as i64 * cy as i64;
                if dist > r2 {
                    let px = corner.0 + dx;
                    let py = corner.1 + dy;
                    if px < w && py < h {
                        let idx = ((py * w + px) * 4) as usize;
                        if idx + 3 < rgba.len() {
                            rgba[idx + 3] = 0;
                        }
                    }
                }
            }
        }
    }
}
