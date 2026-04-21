//! Touch overlay: draw a visual indicator at the mouse position on the mirrored screen.
//!
//! When the user clicks/drags on the display window, this module draws:
//!   - A ripple animation at click position (simulates iOS tap feedback)
//!   - A trail line during drag (simulates swipe path)
//!   - A pulsing ring during long-press
//!
//! The overlay is composited on top of the mirrored frame before display.

/// Overlay element to draw on the frame.
#[derive(Clone, Debug)]
pub enum OverlayElement {
    /// Tap ripple at (x, y) with age in frames (0 = just created, fades after ~15 frames)
    TapRipple { x: u32, y: u32, age: u8 },
    /// Swipe trail: list of points
    SwipeTrail { points: Vec<(u32, u32)> },
    /// Long-press ring at (x, y) with progress 0.0-1.0
    LongPressRing { x: u32, y: u32, progress: f32 },
}

/// Composites overlay elements onto an RGBA frame buffer (in-place).
pub fn draw_overlays(rgba: &mut [u8], width: u32, height: u32, elements: &[OverlayElement]) {
    for elem in elements {
        match elem {
            OverlayElement::TapRipple { x, y, age } => {
                let alpha = 1.0 - (*age as f32 / 15.0);
                if alpha > 0.0 {
                    let radius = 10 + (*age as u32) * 3;
                    draw_circle(rgba, width, height, *x, *y, radius, [100, 180, 255], alpha);
                }
            }
            OverlayElement::SwipeTrail { points } => {
                for window in points.windows(2) {
                    draw_line(
                        rgba,
                        width,
                        height,
                        window[0],
                        window[1],
                        [255, 255, 100],
                        0.6,
                    );
                }
            }
            OverlayElement::LongPressRing { x, y, progress } => {
                let radius = 20;
                draw_circle(
                    rgba,
                    width,
                    height,
                    *x,
                    *y,
                    radius,
                    [255, 100, 100],
                    *progress,
                );
            }
        }
    }
}

fn draw_circle(
    rgba: &mut [u8],
    w: u32,
    h: u32,
    cx: u32,
    cy: u32,
    r: u32,
    color: [u8; 3],
    alpha: f32,
) {
    let r2 = (r * r) as i64;
    let ri2 = ((r.saturating_sub(2)) * (r.saturating_sub(2))) as i64;

    let x_min = cx.saturating_sub(r) as usize;
    let x_max = (cx + r).min(w - 1) as usize;
    let y_min = cy.saturating_sub(r) as usize;
    let y_max = (cy + r).min(h - 1) as usize;

    for y in y_min..=y_max {
        for x in x_min..=x_max {
            let dx = x as i64 - cx as i64;
            let dy = y as i64 - cy as i64;
            let dist2 = dx * dx + dy * dy;
            if dist2 <= r2 && dist2 >= ri2 {
                let idx = (y * w as usize + x) * 4;
                if idx + 2 < rgba.len() {
                    rgba[idx] = blend(rgba[idx], color[0], alpha);
                    rgba[idx + 1] = blend(rgba[idx + 1], color[1], alpha);
                    rgba[idx + 2] = blend(rgba[idx + 2], color[2], alpha);
                }
            }
        }
    }
}

fn draw_line(
    rgba: &mut [u8],
    w: u32,
    h: u32,
    from: (u32, u32),
    to: (u32, u32),
    color: [u8; 3],
    alpha: f32,
) {
    // Bresenham's line algorithm
    let (mut x0, mut y0) = (from.0 as i32, from.1 as i32);
    let (x1, y1) = (to.0 as i32, to.1 as i32);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if x0 >= 0 && x0 < w as i32 && y0 >= 0 && y0 < h as i32 {
            let idx = (y0 as usize * w as usize + x0 as usize) * 4;
            if idx + 2 < rgba.len() {
                rgba[idx] = blend(rgba[idx], color[0], alpha);
                rgba[idx + 1] = blend(rgba[idx + 1], color[1], alpha);
                rgba[idx + 2] = blend(rgba[idx + 2], color[2], alpha);
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn blend(bg: u8, fg: u8, alpha: f32) -> u8 {
    ((bg as f32 * (1.0 - alpha)) + (fg as f32 * alpha)).clamp(0.0, 255.0) as u8
}
