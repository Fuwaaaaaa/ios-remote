//! Procedural iPhone-shaped mock screen used by `--synthetic` mode.
//!
//! Reuses the 5x7 bitmap font from `crate::features::stats_overlay` so we
//! don't ship a second glyph table.

use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

use super::SyntheticDeviceInfo;
use crate::features::stats_overlay::draw_text;

#[derive(Debug)]
pub struct SyntheticFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub timestamp_us: u64,
}

// Font advance is 6px per char in stats_overlay::draw_text; chars are 5px wide.
const GLYPH_ADVANCE: u32 = 6;
const STATUS_BAR_H: u32 = 44;
const ICON_SIZE: u32 = 60;
const ICON_PAD_X: u32 = 18;
const ICON_PAD_Y: u32 = 20;
const GRID_COLS: u32 = 4;
const GRID_ROWS: u32 = 6;
const GRID_TOP: u32 = STATUS_BAR_H + 40;

fn text_width(text: &str) -> u32 {
    text.chars().count() as u32 * GLYPH_ADVANCE
}

/// Render one synthetic frame. `elapsed` drives the clock and animation;
/// `frame_no` drives the counter overlay.
pub fn render_frame(
    info: &SyntheticDeviceInfo,
    elapsed: Duration,
    frame_no: u64,
) -> SyntheticFrame {
    let w = info.width;
    let h = info.height;
    let mut rgba = vec![0u8; (w * h * 4) as usize];

    draw_wallpaper(&mut rgba, w, h);
    draw_status_bar(&mut rgba, w, h, elapsed);
    draw_app_grid(&mut rgba, w, h);
    maybe_draw_notification(&mut rgba, w, h, elapsed);
    draw_frame_counter(&mut rgba, w, h, frame_no);

    SyntheticFrame {
        width: w,
        height: h,
        rgba,
        timestamp_us: elapsed.as_micros() as u64,
    }
}

fn put(rgba: &mut [u8], w: u32, x: u32, y: u32, c: [u8; 4]) {
    let i = ((y * w + x) * 4) as usize;
    if i + 3 < rgba.len() {
        rgba[i] = c[0];
        rgba[i + 1] = c[1];
        rgba[i + 2] = c[2];
        rgba[i + 3] = c[3];
    }
}

fn fill_rect(rgba: &mut [u8], w: u32, h: u32, x: u32, y: u32, rw: u32, rh: u32, c: [u8; 4]) {
    let xe = (x + rw).min(w);
    let ye = (y + rh).min(h);
    for yy in y..ye {
        for xx in x..xe {
            put(rgba, w, xx, yy, c);
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn draw_wallpaper(rgba: &mut [u8], w: u32, h: u32) {
    // Vertical gradient: deep navy (top) → dark purple (bottom)
    for y in 0..h {
        let t = y as f32 / h as f32;
        let r = lerp(10.0, 50.0, t) as u8;
        let g = lerp(15.0, 20.0, t) as u8;
        let b = lerp(40.0, 70.0, t) as u8;
        for x in 0..w {
            put(rgba, w, x, y, [r, g, b, 255]);
        }
    }
}

fn draw_status_bar(rgba: &mut [u8], w: u32, h: u32, elapsed: Duration) {
    // Black bar at top
    fill_rect(rgba, w, h, 0, 0, w, STATUS_BAR_H, [0, 0, 0, 255]);
    // Clock — derived from elapsed so tests are deterministic. Real wall-clock
    // is fine too but we'd lose reproducibility.
    let secs = elapsed.as_secs();
    let hh = (9 + secs / 3600) % 24;
    let mm = (41 + secs / 60) % 60;
    let clock = format!("{:02}:{:02}", hh, mm);
    draw_text(rgba, w, 14, 18, &clock, [0xFF, 0xFF, 0xFF]);
    // "LTE" mid status bar
    draw_text(rgba, w, 100, 18, "LTE", [0xC8, 0xC8, 0xC8]);
    // Battery percent + small battery rectangle right-aligned
    let pct_text = "100%";
    let pct_w = text_width(pct_text);
    draw_text(
        rgba,
        w,
        w.saturating_sub(pct_w + 28),
        18,
        pct_text,
        [0xFF, 0xFF, 0xFF],
    );
    // Battery outline + fill
    fill_rect(rgba, w, h, w - 22, 19, 16, 9, [0xFF, 0xFF, 0xFF, 0xFF]);
    fill_rect(rgba, w, h, w - 21, 20, 14, 7, [0x32, 0xC8, 0x50, 0xFF]);
    // tip
    fill_rect(rgba, w, h, w - 6, 21, 2, 5, [0xFF, 0xFF, 0xFF, 0xFF]);
}

fn draw_app_grid(rgba: &mut [u8], w: u32, h: u32) {
    let palette: [[u8; 4]; 8] = [
        [220, 60, 60, 255],
        [60, 180, 80, 255],
        [60, 120, 220, 255],
        [220, 180, 50, 255],
        [180, 80, 200, 255],
        [240, 130, 50, 255],
        [50, 200, 220, 255],
        [200, 200, 200, 255],
    ];
    let labels = "ABCDEFGHIJKLMNOPQRSTUVWX";
    let mut k = 0usize;
    let total_grid_w = GRID_COLS * ICON_SIZE + (GRID_COLS - 1) * ICON_PAD_X;
    let left = (w - total_grid_w) / 2;
    for row in 0..GRID_ROWS {
        for col in 0..GRID_COLS {
            let x = left + col * (ICON_SIZE + ICON_PAD_X);
            let y = GRID_TOP + row * (ICON_SIZE + ICON_PAD_Y);
            let color = palette[k % palette.len()];
            fill_rect(rgba, w, h, x, y, ICON_SIZE, ICON_SIZE, color);
            // Letter label centered roughly (5px wide × 7px tall char at 1x)
            let label = labels.chars().nth(k).unwrap_or('?');
            let mut buf = [0u8; 4];
            let label_str = label.encode_utf8(&mut buf);
            draw_text(
                rgba,
                w,
                x + ICON_SIZE / 2 - 3,
                y + ICON_SIZE / 2 - 4,
                label_str,
                [0xFF, 0xFF, 0xFF],
            );
            k += 1;
        }
    }
}

fn maybe_draw_notification(rgba: &mut [u8], w: u32, h: u32, elapsed: Duration) {
    // Show banner for 5s every 30s starting at t=30s
    let secs = elapsed.as_secs();
    if secs < 30 {
        return;
    }
    let phase = secs % 30;
    if !(0..5).contains(&phase) {
        return;
    }
    let messages = [
        "HELLO WORLD",
        "NEW MESSAGE FROM A",
        "BATTERY 100 PERCENT",
        "MEETING IN 5 MIN",
    ];
    let idx = ((secs / 30) as usize) % messages.len();
    let msg = messages[idx];
    let banner_y = 100u32;
    let banner_h = 60u32;
    fill_rect(
        rgba,
        w,
        h,
        20,
        banner_y,
        w - 40,
        banner_h,
        [245, 245, 250, 255],
    );
    // dark border row
    fill_rect(rgba, w, h, 20, banner_y, w - 40, 1, [180, 180, 190, 255]);
    fill_rect(
        rgba,
        w,
        h,
        20,
        banner_y + banner_h - 1,
        w - 40,
        1,
        [180, 180, 190, 255],
    );
    draw_text(
        rgba,
        w,
        32,
        banner_y + 10,
        "NOTIFICATION",
        [0x50, 0x50, 0x50],
    );
    draw_text(rgba, w, 32, banner_y + 30, msg, [0x14, 0x14, 0x14]);
}

fn draw_frame_counter(rgba: &mut [u8], w: u32, h: u32, frame_no: u64) {
    let text = format!("SYNTH {:06}", frame_no);
    let tw = text_width(&text);
    let x = w.saturating_sub(tw + 6);
    let y = h.saturating_sub(14);
    fill_rect(
        rgba,
        w,
        h,
        x.saturating_sub(2),
        y.saturating_sub(2),
        tw + 4,
        12,
        [0, 0, 0, 200],
    );
    draw_text(rgba, w, x, y, &text, [0x78, 0xFF, 0x78]);
}

/// Spawn a tokio task that renders frames at ~30 FPS and pushes them through
/// `publish`. Caller holds the JoinHandle for lifetime control.
pub fn spawn_frame_loop(
    info: SyntheticDeviceInfo,
    publish: Arc<dyn Fn(SyntheticFrame) + Send + Sync>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let start = std::time::Instant::now();
        let mut frame_no: u64 = 0;
        let mut tick = tokio::time::interval(Duration::from_millis(33));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let elapsed = start.elapsed();
            let frame = render_frame(&info, elapsed, frame_no);
            publish(frame);
            frame_no = frame_no.wrapping_add(1);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_at(t_ms: u64) -> SyntheticFrame {
        let info = SyntheticDeviceInfo::default_iphone_15();
        render_frame(&info, Duration::from_millis(t_ms), t_ms / 33)
    }

    #[test]
    fn render_frame_dimensions_match_iphone_15() {
        let frame = render_at(0);
        assert_eq!(frame.width, 390);
        assert_eq!(frame.height, 844);
        assert_eq!(frame.rgba.len(), (390 * 844 * 4) as usize);
    }

    #[test]
    fn status_bar_top_strip_is_dark() {
        // Top 4 rows average luminance should be very low (status bar = black)
        let frame = render_at(0);
        let mut sum: u64 = 0;
        for y in 0..4u32 {
            for x in 0..390u32 {
                let i = ((y * 390 + x) * 4) as usize;
                sum += frame.rgba[i] as u64 + frame.rgba[i + 1] as u64 + frame.rgba[i + 2] as u64;
            }
        }
        let avg = sum / (4 * 390 * 3);
        assert!(avg < 80, "status bar should be dark (avg<80), got {avg}");
    }

    #[test]
    fn frame_counter_increments_visibly_between_frames() {
        // The bottom-right frame counter text differs between two adjacent frames
        let f0 = render_at(0);
        let f100 = render_at(100);
        let mut diff = 0u32;
        for y in 824..844u32 {
            for x in 290..390u32 {
                let i = ((y * 390 + x) * 4) as usize;
                if f0.rgba[i] != f100.rgba[i] || f0.rgba[i + 1] != f100.rgba[i + 1] {
                    diff += 1;
                }
            }
        }
        assert!(
            diff > 0,
            "frame counter area should change between frames, diff={diff}"
        );
    }

    #[test]
    fn app_grid_pixels_are_brighter_than_wallpaper() {
        // App-icon mid row should have many bright pixels (rgb sum > 300)
        let frame = render_at(0);
        // GRID_TOP=84, ICON_SIZE=60 → row 84..144 has the first row of icons.
        let mid_row = 110u32;
        let mut bright_count = 0;
        for x in 0..390u32 {
            let i = ((mid_row * 390 + x) * 4) as usize;
            let lum = frame.rgba[i] as u32 + frame.rgba[i + 1] as u32 + frame.rgba[i + 2] as u32;
            if lum > 300 {
                bright_count += 1;
            }
        }
        assert!(
            bright_count > 40,
            "app grid row should have many bright pixels, got {bright_count}"
        );
    }

    #[test]
    fn notification_banner_visible_around_t_30s_window() {
        // Brightness in the banner band (y=100..160) should jump when banner is shown
        let off = render_at(15_000); // 15s — banner not active
        let on = render_at(32_000); // 32s — banner active
        let avg_band = |f: &SyntheticFrame| -> u32 {
            let mut sum = 0u64;
            for y in 100..160u32 {
                for x in 20..370u32 {
                    let i = ((y * 390 + x) * 4) as usize;
                    sum += f.rgba[i] as u64 + f.rgba[i + 1] as u64 + f.rgba[i + 2] as u64;
                }
            }
            (sum / (60 * 350 * 3)) as u32
        };
        let on_avg = avg_band(&on);
        let off_avg = avg_band(&off);
        assert!(
            on_avg > off_avg + 30,
            "banner should be brighter at t=32s than t=15s (on={on_avg}, off={off_avg})"
        );
    }

    #[test]
    fn alpha_channel_is_set_to_255_everywhere() {
        // Wallpaper fill ensures alpha=255 across the buffer. Important because
        // the recording / replay pipeline consumes alpha for some operations.
        let frame = render_at(0);
        for y in (0..844u32).step_by(50) {
            for x in (0..390u32).step_by(50) {
                let i = ((y * 390 + x) * 4) as usize;
                assert_eq!(frame.rgba[i + 3], 255, "alpha at ({x},{y}) should be 255");
            }
        }
    }
}
