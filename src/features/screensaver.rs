use chrono::Local;

/// Screensaver: display clock/info when no device is connected.
pub struct Screensaver {
    pub enabled: bool,
    pub mode: ScreensaverMode,
}

#[derive(Clone, Debug)]
pub enum ScreensaverMode {
    Clock,
    ConnectionInfo { ip: String, port: u16 },
    Blank,
}

impl Screensaver {
    pub fn new() -> Self {
        Self { enabled: true, mode: ScreensaverMode::Clock }
    }

    /// Render screensaver frame as RGBA buffer.
    pub fn render(&self, width: u32, height: u32) -> Vec<u8> {
        let mut rgba = vec![0x1Au8; (width * height * 4) as usize]; // dark bg
        // Set alpha
        for i in 0..(width * height) as usize { rgba[i * 4 + 3] = 255; }

        match &self.mode {
            ScreensaverMode::Clock => {
                let time_str = Local::now().format("%H:%M:%S").to_string();
                let date_str = Local::now().format("%Y-%m-%d").to_string();
                // Center the clock text (large)
                let cx = width / 2 - (time_str.len() as u32 * 20) / 2;
                let cy = height / 2 - 20;
                draw_large_text(&mut rgba, width, cx, cy, &time_str, [0x00, 0xD4, 0xFF]);
                let dx = width / 2 - (date_str.len() as u32 * 8) / 2;
                draw_small_text(&mut rgba, width, dx, cy + 50, &date_str, [0x88, 0x88, 0x88]);
            }
            ScreensaverMode::ConnectionInfo { ip, port } => {
                let msg = format!("ios-remote: {}:{}", ip, port);
                let hint = "Connect your iPhone to start mirroring";
                let cx = width / 2 - (msg.len() as u32 * 8) / 2;
                draw_small_text(&mut rgba, width, cx, height / 2 - 20, &msg, [0x00, 0xD4, 0xFF]);
                let hx = width / 2 - (hint.len() as u32 * 6) / 2;
                draw_small_text(&mut rgba, width, hx, height / 2 + 20, hint, [0x66, 0x66, 0x66]);
            }
            ScreensaverMode::Blank => {}
        }

        rgba
    }
}

fn draw_large_text(rgba: &mut [u8], w: u32, x: u32, y: u32, text: &str, color: [u8; 3]) {
    // 3x scale block font
    for (i, _ch) in text.chars().enumerate() {
        let px = x + i as u32 * 20;
        for dy in 0..24u32 {
            for dx in 0..16u32 {
                if px + dx < w {
                    let idx = (((y + dy) * w + px + dx) * 4) as usize;
                    if idx + 2 < rgba.len() && (dy % 3 != 0 || dx % 3 != 0) {
                        rgba[idx] = color[0]; rgba[idx + 1] = color[1]; rgba[idx + 2] = color[2];
                    }
                }
            }
        }
    }
}

fn draw_small_text(rgba: &mut [u8], w: u32, x: u32, y: u32, text: &str, color: [u8; 3]) {
    for (i, _ch) in text.chars().enumerate() {
        let px = x + i as u32 * 7;
        for dy in 0..7u32 {
            for dx in 0..5u32 {
                if px + dx < w {
                    let idx = (((y + dy) * w + px + dx) * 4) as usize;
                    if idx + 2 < rgba.len() {
                        rgba[idx] = color[0]; rgba[idx + 1] = color[1]; rgba[idx + 2] = color[2];
                    }
                }
            }
        }
    }
}
