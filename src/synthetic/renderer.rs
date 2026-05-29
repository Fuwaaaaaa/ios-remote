//! Procedural iPhone-shaped mock screen used by `--synthetic` mode.
//!
//! The renderer is now *state-driven*: each frame it reads a [`StateSnapshot`]
//! (produced under lock by the WDA stub's input handlers) and draws the
//! current screen — the home grid (with its current page) or an opened app
//! view. Tapping an icon, swiping pages, and the home/back gesture are all
//! reflected here, so screenshots / OCR / recording capture real interaction.
//!
//! Reuses the 5x7 bitmap font from `crate::features::stats_overlay` so we
//! don't ship a second glyph table. Geometry constants and hit-testing live in
//! [`super::layout`] so the drawn icons and the tappable hit-boxes can't drift.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

use super::SyntheticDeviceInfo;
use super::layout::{self, GLYPH_ADVANCE, GRID_COLS, GRID_ROWS, GRID_TOP, ICON_SIZE, STATUS_BAR_H};
use super::state::{FLASH_DURATION_US, InputKind, Screen, SharedState, StateSnapshot};
use crate::features::stats_overlay::draw_text;

#[derive(Debug)]
pub struct SyntheticFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub timestamp_us: u64,
}

/// 8-colour icon palette, indexed by global app index.
const PALETTE: [[u8; 4]; 8] = [
    [220, 60, 60, 255],
    [60, 180, 80, 255],
    [60, 120, 220, 255],
    [220, 180, 50, 255],
    [180, 80, 200, 255],
    [240, 130, 50, 255],
    [50, 200, 220, 255],
    [200, 200, 200, 255],
];

fn text_width(text: &str) -> u32 {
    text.chars().count() as u32 * GLYPH_ADVANCE
}

/// Render one synthetic frame from the current device state. `elapsed` drives
/// the clock / animation; `frame_no` drives the counter overlay.
pub fn render_frame(
    info: &SyntheticDeviceInfo,
    snap: &StateSnapshot,
    elapsed: Duration,
    frame_no: u64,
) -> SyntheticFrame {
    let w = info.width;
    let h = info.height;
    let mut rgba = vec![0u8; (w * h * 4) as usize];

    draw_wallpaper(&mut rgba, w, h);
    draw_status_bar(&mut rgba, w, h, elapsed);

    match snap.screen {
        Screen::Home => {
            draw_app_grid(&mut rgba, w, h, snap.home_page);
            draw_page_dots(&mut rgba, w, h, snap.home_page);
            maybe_draw_notification(&mut rgba, w, h, elapsed);
        }
        Screen::App { index } => {
            draw_app_view(&mut rgba, w, h, index, snap.app_scroll);
        }
    }

    draw_long_press_flash(&mut rgba, w, h, snap, elapsed.as_micros() as u64);
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
    // Battery percent + small battery rectangle right-aligned. saturating_sub
    // keeps a degenerate narrow device from underflowing (I2).
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
    fill_rect(
        rgba,
        w,
        h,
        w.saturating_sub(22),
        19,
        16,
        9,
        [0xFF, 0xFF, 0xFF, 0xFF],
    );
    fill_rect(
        rgba,
        w,
        h,
        w.saturating_sub(21),
        20,
        14,
        7,
        [0x32, 0xC8, 0x50, 0xFF],
    );
    // tip
    fill_rect(
        rgba,
        w,
        h,
        w.saturating_sub(6),
        21,
        2,
        5,
        [0xFF, 0xFF, 0xFF, 0xFF],
    );
}

fn draw_app_grid(rgba: &mut [u8], w: u32, h: u32, home_page: u32) {
    for row in 0..GRID_ROWS {
        for col in 0..GRID_COLS {
            let slot = row * GRID_COLS + col;
            let index = layout::global_app_index(home_page, slot);
            let rect = layout::icon_rect(w, row, col);
            let color = PALETTE[index as usize % PALETTE.len()];
            fill_rect(rgba, w, h, rect.x, rect.y, rect.w, rect.h, color);
            // Letter label centered roughly (5px wide × 7px tall char at 1x)
            let label = layout::app_letter(index);
            let mut buf = [0u8; 4];
            let label_str = label.encode_utf8(&mut buf);
            draw_text(
                rgba,
                w,
                rect.x + ICON_SIZE / 2 - 3,
                rect.y + ICON_SIZE / 2 - 4,
                label_str,
                [0xFF, 0xFF, 0xFF],
            );
        }
    }
}

/// Small page-indicator dots below the grid; the current page is brighter.
fn draw_page_dots(rgba: &mut [u8], w: u32, h: u32, home_page: u32) {
    let pages = super::state::HOME_PAGES;
    let dot = 6u32;
    let gap = 10u32;
    let total = pages * dot + (pages - 1) * gap;
    let mut x = w.saturating_sub(total) / 2;
    let y = GRID_TOP + GRID_ROWS * (ICON_SIZE + layout::ICON_PAD_Y) + 16;
    for p in 0..pages {
        let c = if p == home_page {
            [0xFF, 0xFF, 0xFF, 0xFF]
        } else {
            [0x70, 0x70, 0x80, 0xFF]
        };
        fill_rect(rgba, w, h, x, y, dot, dot, c);
        x += dot + gap;
    }
}

/// Opened-app screen: title bar with a back chevron + app letter, a scrollable
/// striped content area, and a bottom home indicator.
fn draw_app_view(rgba: &mut [u8], w: u32, h: u32, index: u32, scroll: i32) {
    let title_h = 44u32;
    let title_top = STATUS_BAR_H;
    let base = PALETTE[index as usize % PALETTE.len()];

    // Title bar tinted with the app colour.
    let title_c = [
        (base[0] as u32 * 2 / 3) as u8,
        (base[1] as u32 * 2 / 3) as u8,
        (base[2] as u32 * 2 / 3) as u8,
        255,
    ];
    fill_rect(rgba, w, h, 0, title_top, w, title_h, title_c);

    // Back chevron in the back_zone (top-left).
    let bz = layout::back_zone();
    draw_text(rgba, w, bz.x + 14, bz.y + 16, "<", [0xFF, 0xFF, 0xFF]);

    // App title centred.
    let letter = layout::app_letter(index);
    let title = format!("APP {}", letter);
    let tw = text_width(&title);
    draw_text(
        rgba,
        w,
        w.saturating_sub(tw) / 2,
        title_top + 16,
        &title,
        [0xFF, 0xFF, 0xFF],
    );

    // Scrollable striped content band.
    let content_top = title_top + title_h;
    let content_bot = h.saturating_sub(layout::home_indicator_zone(w, h).h + 4);
    let stripe_h = 48u32;
    let period = stripe_h * 2;
    let offset = (scroll.max(0) as u32) % period;
    for y in content_top..content_bot {
        let rel = (y - content_top + offset) % period;
        let band = rel / stripe_h;
        let c = if band == 0 {
            [
                (base[0] / 2).saturating_add(40),
                (base[1] / 2).saturating_add(40),
                (base[2] / 2).saturating_add(40),
                255,
            ]
        } else {
            [
                (base[0] / 3).saturating_add(20),
                (base[1] / 3).saturating_add(20),
                (base[2] / 3).saturating_add(20),
                255,
            ]
        };
        for x in 0..w {
            put(rgba, w, x, y, c);
        }
    }

    // Home indicator bar (tap target to return home).
    let hz = layout::home_indicator_zone(w, h);
    fill_rect(
        rgba,
        w,
        h,
        hz.x,
        hz.y + hz.h / 2,
        hz.w,
        5,
        [0xE0, 0xE0, 0xE0, 0xFF],
    );
}

fn draw_long_press_flash(rgba: &mut [u8], w: u32, h: u32, snap: &StateSnapshot, now_us: u64) {
    if snap.last_input_kind != InputKind::LongPress {
        return;
    }
    if now_us < snap.last_input_at_us || now_us - snap.last_input_at_us >= FLASH_DURATION_US {
        return;
    }
    // Hollow white square centred on the press point.
    let half = 22u32;
    let cx = snap.last_input_x;
    let cy = snap.last_input_y;
    let x0 = cx.saturating_sub(half);
    let y0 = cy.saturating_sub(half);
    let side = half * 2;
    let c = [0xFF, 0xFF, 0xFF, 0xFF];
    fill_rect(rgba, w, h, x0, y0, side, 2, c); // top
    fill_rect(rgba, w, h, x0, y0 + side, side, 2, c); // bottom
    fill_rect(rgba, w, h, x0, y0, 2, side, c); // left
    fill_rect(rgba, w, h, x0 + side, y0, 2, side + 2, c); // right
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

/// Spawn a tokio task that renders frames at ~30 FPS from the shared device
/// state and pushes them through `publish`. `start` is the shared monotonic
/// clock (also used by the WDA stub to timestamp input) so the long-press
/// flash decays correctly. Caller holds the JoinHandle for lifetime control.
pub fn spawn_frame_loop(
    info: SyntheticDeviceInfo,
    state: SharedState,
    start: Arc<Instant>,
    publish: Arc<dyn Fn(SyntheticFrame) + Send + Sync>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut frame_no: u64 = 0;
        let mut tick = tokio::time::interval(Duration::from_millis(33));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let elapsed = start.elapsed();
            // Lock only to take an O(1) snapshot; drop the guard before any
            // pixel work so the guard never crosses an `.await`.
            let snap = {
                let guard = state.lock().unwrap_or_else(|e| e.into_inner());
                guard.snapshot()
            };
            let frame = render_frame(&info, &snap, elapsed, frame_no);
            publish(frame);
            frame_no = frame_no.wrapping_add(1);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::state::DeviceState;

    fn render_at(t_ms: u64) -> SyntheticFrame {
        let info = SyntheticDeviceInfo::default_iphone_15();
        let snap = DeviceState::new(info.width, info.height).snapshot();
        render_frame(&info, &snap, Duration::from_millis(t_ms), t_ms / 33)
    }

    fn render_state(snap: &StateSnapshot, t_ms: u64) -> SyntheticFrame {
        let info = SyntheticDeviceInfo::default_iphone_15();
        render_frame(&info, snap, Duration::from_millis(t_ms), t_ms / 33)
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

    #[test]
    fn home_and_app_views_differ_in_content_band() {
        // Same time, different screen: the content band (y 200..600) must differ
        // substantially between the home grid and an opened app view.
        let mut home = DeviceState::new(390, 844);
        let home_snap = home.snapshot();
        home.screen = Screen::App { index: 0 };
        let app_snap = home.snapshot();

        let f_home = render_state(&home_snap, 1000);
        let f_app = render_state(&app_snap, 1000);

        let mut diff = 0u32;
        for y in (200..600u32).step_by(3) {
            for x in (0..390u32).step_by(3) {
                let i = ((y * 390 + x) * 4) as usize;
                if f_home.rgba[i] != f_app.rgba[i] || f_home.rgba[i + 2] != f_app.rgba[i + 2] {
                    diff += 1;
                }
            }
        }
        assert!(diff > 500, "home vs app content should differ, diff={diff}");
    }

    #[test]
    fn narrow_device_does_not_panic() {
        // I2: a degenerate device narrower than the grid must not underflow.
        let info = SyntheticDeviceInfo {
            width: 10,
            height: 50,
            ..SyntheticDeviceInfo::default_iphone_15()
        };
        let snap = DeviceState::new(10, 50).snapshot();
        let frame = render_frame(&info, &snap, Duration::from_millis(0), 0);
        assert_eq!(frame.rgba.len(), (10 * 50 * 4) as usize);
    }
}
