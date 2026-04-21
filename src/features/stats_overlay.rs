/// Stats overlay: render FPS, latency, resolution info on top of the frame.
///
/// Drawn as a semi-transparent bar at the top of the display window.

#[derive(Clone, Debug)]
pub struct Stats {
    pub fps: f64,
    pub frame_count: u64,
    pub latency_ms: f64,
    pub resolution: (u32, u32),
    pub bitrate_kbps: f64,
}

/// Draw stats text onto an RGBA buffer (simple bitmap font).
pub fn draw_stats_overlay(rgba: &mut [u8], width: u32, _height: u32, stats: &Stats) {
    let text = format!(
        " {:.0} FPS | {}x{} | {:.0}ms | {:.0} kbps | {} frames ",
        stats.fps,
        stats.resolution.0,
        stats.resolution.1,
        stats.latency_ms,
        stats.bitrate_kbps,
        stats.frame_count,
    );

    // Draw semi-transparent dark bar at top (24px tall)
    let bar_height = 24u32;
    for y in 0..bar_height.min(_height) {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            if idx + 3 < rgba.len() {
                rgba[idx] = (rgba[idx] as u16 * 40 / 100) as u8; // R dimmed
                rgba[idx + 1] = (rgba[idx + 1] as u16 * 40 / 100) as u8; // G dimmed
                rgba[idx + 2] = (rgba[idx + 2] as u16 * 40 / 100) as u8; // B dimmed
            }
        }
    }

    // Draw text characters (simple 5x7 bitmap font)
    let start_x = 8u32;
    let start_y = 8u32;
    for (i, ch) in text.chars().enumerate() {
        let x = start_x + (i as u32) * 6;
        if x + 5 >= width {
            break;
        }
        draw_char(rgba, width, x, start_y, ch);
    }
}

/// Draw a single character using a minimal 5x7 bitmap font.
fn draw_char(rgba: &mut [u8], width: u32, x: u32, y: u32, ch: char) {
    let bitmap = char_bitmap(ch);
    for row in 0..7u32 {
        for col in 0..5u32 {
            if (bitmap[row as usize] >> (4 - col)) & 1 == 1 {
                let px = x + col;
                let py = y + row;
                let idx = ((py * width + px) * 4) as usize;
                if idx + 2 < rgba.len() {
                    rgba[idx] = 0x00; // R
                    rgba[idx + 1] = 0xD4; // G (cyan)
                    rgba[idx + 2] = 0xFF; // B
                }
            }
        }
    }
}

/// Minimal 5x7 bitmap font for digits, letters, and common symbols.
fn char_bitmap(ch: char) -> [u8; 7] {
    match ch {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
        '3' => [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'S' => [0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E],
        'k' => [0x10, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'b' => [0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x1E],
        'p' => [0x00, 0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10],
        's' => [0x00, 0x0F, 0x10, 0x0E, 0x01, 0x1E, 0x00],
        'm' => [0x00, 0x1A, 0x15, 0x15, 0x11, 0x11, 0x11],
        'x' => [0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x00],
        'f' => [0x06, 0x08, 0x1C, 0x08, 0x08, 0x08, 0x08],
        'r' => [0x00, 0x16, 0x19, 0x10, 0x10, 0x10, 0x10],
        'a' => [0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F, 0x00],
        'e' => [0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E, 0x00],
        '|' => [0x04, 0x04, 0x04, 0x00, 0x04, 0x04, 0x04],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04],
        _ => [0x1F, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1F], // box for unknown
    }
}
