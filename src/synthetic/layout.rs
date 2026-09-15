//! Single source of truth for the synthetic home-screen geometry.
//!
//! Both the renderer (`renderer.rs`, which *draws* the app grid) and the WDA
//! stub (`wda_stub.rs`, which *hit-tests* incoming taps) consume these
//! constants and helpers. Keeping them here means the drawn icon rectangles
//! and the tappable hit-boxes can never drift apart — a tap at the centre of
//! a drawn icon always opens that icon's app.
//!
//! Coordinate space: WDA input coordinates are 1:1 with frame pixels for the
//! synthetic device (no point→pixel scaling), so hit-testing is direct.

pub const STATUS_BAR_H: u32 = 44;
pub const ICON_SIZE: u32 = 60;
pub const ICON_PAD_X: u32 = 18;
pub const ICON_PAD_Y: u32 = 20;
pub const GRID_COLS: u32 = 4;
pub const GRID_ROWS: u32 = 6;
pub const GRID_TOP: u32 = STATUS_BAR_H + 40;
/// `stats_overlay::draw_text` advances 6px per glyph (5px glyph + 1px gap).
pub const GLYPH_ADVANCE: u32 = 6;
/// Icons per home page.
pub const SLOTS_PER_PAGE: u32 = GRID_COLS * GRID_ROWS;

/// An axis-aligned rectangle in frame-pixel space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IconRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl IconRect {
    /// Half-open containment: `[x, x+w)` × `[y, y+h)`.
    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Total width spanned by the grid (icons + inter-icon padding).
fn total_grid_w() -> u32 {
    GRID_COLS * ICON_SIZE + (GRID_COLS - 1) * ICON_PAD_X
}

/// Left edge of the centred app grid. Uses `saturating_sub` so a degenerate
/// device narrower than the grid never underflows (fixes I2 at the source —
/// the renderer and hit-tester both route through here).
pub fn grid_left(w: u32) -> u32 {
    w.saturating_sub(total_grid_w()) / 2
}

/// Rectangle of the icon at `(row, col)` on the current page.
pub fn icon_rect(w: u32, row: u32, col: u32) -> IconRect {
    IconRect {
        x: grid_left(w) + col * (ICON_SIZE + ICON_PAD_X),
        y: GRID_TOP + row * (ICON_SIZE + ICON_PAD_Y),
        w: ICON_SIZE,
        h: ICON_SIZE,
    }
}

/// Which grid slot (`0..SLOTS_PER_PAGE`) contains `(x, y)`, if any. Taps that
/// land in the inter-icon padding return `None`.
pub fn hit_test_icon(w: u32, x: u32, y: u32) -> Option<u32> {
    for row in 0..GRID_ROWS {
        for col in 0..GRID_COLS {
            if icon_rect(w, row, col).contains(x, y) {
                return Some(row * GRID_COLS + col);
            }
        }
    }
    None
}

/// Back-chevron tap target shown in the app-view title bar (top-left).
pub fn back_zone() -> IconRect {
    IconRect {
        x: 0,
        y: STATUS_BAR_H,
        w: 60,
        h: 44,
    }
}

/// Home-indicator tap target at the bottom centre of the app view. Tapping it
/// (like the iPhone home bar) returns to the home screen.
pub fn home_indicator_zone(w: u32, h: u32) -> IconRect {
    let bar_w = 140u32.min(w);
    IconRect {
        x: w.saturating_sub(bar_w) / 2,
        y: h.saturating_sub(28),
        w: bar_w,
        h: 28,
    }
}

/// Map a page + slot to a stable global app index.
pub fn global_app_index(home_page: u32, slot: u32) -> u32 {
    home_page * SLOTS_PER_PAGE + slot
}

/// Letter label for an app index. Wraps A–Z so any index is valid (never
/// panics, unlike indexing a fixed label string).
pub fn app_letter(index: u32) -> char {
    (b'A' + (index % 26) as u8) as char
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: u32 = 390;

    #[test]
    fn hit_test_center_hits_every_cell() {
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
                let r = icon_rect(W, row, col);
                let cx = r.x + r.w / 2;
                let cy = r.y + r.h / 2;
                assert_eq!(
                    hit_test_icon(W, cx, cy),
                    Some(row * GRID_COLS + col),
                    "centre of ({row},{col}) should hit its slot"
                );
            }
        }
    }

    #[test]
    fn hit_test_inter_icon_padding_misses() {
        // A point between icon col 0 and col 1, vertically inside the first row.
        let r0 = icon_rect(W, 0, 0);
        let gap_x = r0.x + ICON_SIZE + ICON_PAD_X / 2;
        let cy = r0.y + r0.h / 2;
        assert_eq!(hit_test_icon(W, gap_x, cy), None);
    }

    #[test]
    fn hit_test_status_bar_misses() {
        assert_eq!(hit_test_icon(W, 10, 10), None);
    }

    #[test]
    fn first_icon_origin_is_centered_grid_left() {
        let r = icon_rect(W, 0, 0);
        assert_eq!(r.x, grid_left(W));
        assert_eq!(r.y, GRID_TOP);
    }

    #[test]
    fn grid_left_does_not_underflow_on_narrow_device() {
        // Narrower than the grid → saturating_sub yields 0, no panic (I2).
        assert_eq!(grid_left(10), 0);
        assert_eq!(grid_left(0), 0);
    }

    #[test]
    fn global_app_index_uses_page_offset() {
        assert_eq!(global_app_index(0, 0), 0);
        assert_eq!(global_app_index(0, 23), 23);
        assert_eq!(global_app_index(1, 0), SLOTS_PER_PAGE);
    }

    #[test]
    fn app_letter_wraps_past_z() {
        assert_eq!(app_letter(0), 'A');
        assert_eq!(app_letter(23), 'X');
        assert_eq!(app_letter(25), 'Z');
        assert_eq!(app_letter(26), 'A');
        assert_eq!(app_letter(u32::MAX), app_letter(u32::MAX % 26));
    }

    #[test]
    fn back_and_home_zones_are_disjoint_from_grid() {
        // Back zone sits in the title-bar band, above the grid.
        assert!(back_zone().y < GRID_TOP);
        // Home indicator sits near the bottom, below the grid.
        assert!(home_indicator_zone(W, 844).y > GRID_TOP + GRID_ROWS * (ICON_SIZE + ICON_PAD_Y));
    }
}
