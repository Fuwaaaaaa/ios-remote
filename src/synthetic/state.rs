//! Shared, interactive device state for `--synthetic` mode.
//!
//! This is what turns synthetic mode from a passive animation into a usable
//! emulator: WDA input (tap / swipe / long-press) mutates a `DeviceState`,
//! and the renderer reads a cheap [`StateSnapshot`] every frame to draw the
//! *current* screen. The loop closes — display → input → state → display — so
//! screenshot / OCR / AI / recording all capture real interaction.
//!
//! Concurrency: `Arc<std::sync::Mutex<DeviceState>>` (the house style, with the
//! `unwrap_or_else(|e| e.into_inner())` poison-recovery idiom). The renderer
//! locks only long enough to take an O(1) [`StateSnapshot`], then drops the
//! guard before any pixel work — so **no guard is ever held across `.await`**.
//! Transitions mutate atomically under the same lock, so the renderer never
//! observes a half-applied transition (worst case it sees the pre- or
//! post-transition state, never a torn one).

use super::layout;

/// Number of swipeable home-screen pages.
pub const HOME_PAGES: u32 = 2;
/// Minimum travel (px) before a swipe is recognised as a page-flip / scroll.
pub const SWIPE_THRESHOLD: i32 = 40;
/// Maximum app-view scroll offset (px).
pub const APP_MAX_SCROLL: i32 = 600;
/// Long-press highlight fade duration (µs).
pub const FLASH_DURATION_US: u64 = 400_000;

/// Which screen is currently shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    App { index: u32 },
}

/// The most recent input gesture (drives the brief long-press highlight).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    None,
    Tap,
    Swipe,
    LongPress,
}

/// Mutable interactive state of the synthetic device.
#[derive(Clone, Debug)]
pub struct DeviceState {
    pub screen: Screen,
    pub home_page: u32,
    pub app_scroll: i32,
    pub last_input_kind: InputKind,
    pub last_input_x: u32,
    pub last_input_y: u32,
    pub last_input_at_us: u64,
    pub interactions: u64,
    /// Device dimensions, used for hit-testing the home grid / home indicator.
    width: u32,
    height: u32,
}

/// Cheap `Copy` view the renderer reads each frame after dropping the lock.
#[derive(Clone, Copy, Debug)]
pub struct StateSnapshot {
    pub screen: Screen,
    pub home_page: u32,
    pub app_scroll: i32,
    pub last_input_kind: InputKind,
    pub last_input_x: u32,
    pub last_input_y: u32,
    pub last_input_at_us: u64,
    pub interactions: u64,
}

impl DeviceState {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            screen: Screen::Home,
            home_page: 0,
            app_scroll: 0,
            last_input_kind: InputKind::None,
            last_input_x: 0,
            last_input_y: 0,
            last_input_at_us: 0,
            interactions: 0,
            width,
            height,
        }
    }

    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            screen: self.screen,
            home_page: self.home_page,
            app_scroll: self.app_scroll,
            last_input_kind: self.last_input_kind,
            last_input_x: self.last_input_x,
            last_input_y: self.last_input_y,
            last_input_at_us: self.last_input_at_us,
            interactions: self.interactions,
        }
    }

    fn note_input(&mut self, kind: InputKind, x: u32, y: u32, now_us: u64) {
        self.last_input_kind = kind;
        self.last_input_x = x;
        self.last_input_y = y;
        self.last_input_at_us = now_us;
        self.interactions += 1;
    }
}

/// Thread-safe handle shared by the renderer loop and the WDA stub.
pub type SharedState = std::sync::Arc<std::sync::Mutex<DeviceState>>;

pub fn new_shared(width: u32, height: u32) -> SharedState {
    std::sync::Arc::new(std::sync::Mutex::new(DeviceState::new(width, height)))
}

/// Lock the shared state with poison recovery and run `f`.
pub fn with_lock<R>(s: &SharedState, f: impl FnOnce(&mut DeviceState) -> R) -> R {
    let mut guard = s.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// Apply a tap. On Home, tapping an icon opens its app; on an app view,
/// tapping the back chevron or home indicator returns home. Misses are no-ops
/// (but still counted as an interaction).
pub fn apply_tap(st: &mut DeviceState, x: u32, y: u32, now_us: u64) {
    st.note_input(InputKind::Tap, x, y, now_us);
    match st.screen {
        Screen::Home => {
            if let Some(slot) = layout::hit_test_icon(st.width, x, y) {
                st.screen = Screen::App {
                    index: layout::global_app_index(st.home_page, slot),
                };
                st.app_scroll = 0;
            }
        }
        Screen::App { .. } => {
            if layout::back_zone().contains(x, y)
                || layout::home_indicator_zone(st.width, st.height).contains(x, y)
            {
                st.screen = Screen::Home;
            }
        }
    }
}

/// Apply a swipe. Horizontal swipes flip home pages; vertical swipes scroll an
/// open app. Sub-threshold / wrong-axis swipes are no-ops.
pub fn apply_swipe(st: &mut DeviceState, x1: u32, y1: u32, x2: u32, y2: u32, now_us: u64) {
    st.note_input(InputKind::Swipe, x2, y2, now_us);
    let dx = x2 as i32 - x1 as i32;
    let dy = y2 as i32 - y1 as i32;
    match st.screen {
        Screen::Home => {
            if dx.abs() > SWIPE_THRESHOLD && dx.abs() > dy.abs() {
                if dx < 0 {
                    st.home_page = (st.home_page + 1).min(HOME_PAGES - 1);
                } else {
                    st.home_page = st.home_page.saturating_sub(1);
                }
            }
        }
        Screen::App { .. } => {
            if dy.abs() > SWIPE_THRESHOLD && dy.abs() >= dx.abs() {
                // Swiping up (dy<0) scrolls content down → larger offset.
                st.app_scroll = (st.app_scroll - dy).clamp(0, APP_MAX_SCROLL);
            }
        }
    }
}

/// Apply a long press. Purely visual: records the gesture so the renderer can
/// draw a brief highlight ring at the press point.
pub fn apply_long_press(st: &mut DeviceState, x: u32, y: u32, now_us: u64) {
    st.note_input(InputKind::LongPress, x, y, now_us);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev() -> DeviceState {
        DeviceState::new(390, 844)
    }

    fn icon_center(row: u32, col: u32) -> (u32, u32) {
        let r = layout::icon_rect(390, row, col);
        (r.x + r.w / 2, r.y + r.h / 2)
    }

    #[test]
    fn tap_first_icon_opens_app_a() {
        let mut st = dev();
        let (x, y) = icon_center(0, 0);
        apply_tap(&mut st, x, y, 1000);
        assert_eq!(st.screen, Screen::App { index: 0 });
        assert_eq!(st.interactions, 1);
    }

    #[test]
    fn tap_in_grid_padding_is_noop_but_counts() {
        let mut st = dev();
        let r0 = layout::icon_rect(390, 0, 0);
        let gap_x = r0.x + layout::ICON_SIZE + layout::ICON_PAD_X / 2;
        apply_tap(&mut st, gap_x, r0.y + 30, 1000);
        assert_eq!(st.screen, Screen::Home);
        assert_eq!(st.interactions, 1);
    }

    #[test]
    fn tap_second_page_icon_uses_page_offset() {
        let mut st = dev();
        st.home_page = 1;
        let (x, y) = icon_center(0, 0);
        apply_tap(&mut st, x, y, 1000);
        assert_eq!(
            st.screen,
            Screen::App {
                index: layout::SLOTS_PER_PAGE
            }
        );
    }

    #[test]
    fn tap_back_zone_returns_home() {
        let mut st = dev();
        st.screen = Screen::App { index: 3 };
        let bz = layout::back_zone();
        apply_tap(&mut st, bz.x + 5, bz.y + 5, 2000);
        assert_eq!(st.screen, Screen::Home);
    }

    #[test]
    fn tap_home_indicator_returns_home() {
        let mut st = dev();
        st.screen = Screen::App { index: 3 };
        let hz = layout::home_indicator_zone(390, 844);
        apply_tap(&mut st, hz.x + hz.w / 2, hz.y + 5, 2000);
        assert_eq!(st.screen, Screen::Home);
    }

    #[test]
    fn tap_in_app_content_is_noop() {
        let mut st = dev();
        st.screen = Screen::App { index: 3 };
        apply_tap(&mut st, 195, 400, 2000);
        assert_eq!(st.screen, Screen::App { index: 3 });
    }

    #[test]
    fn swipe_left_advances_home_page() {
        let mut st = dev();
        apply_swipe(&mut st, 300, 400, 100, 400, 1000);
        assert_eq!(st.home_page, 1);
        // Clamps at the last page.
        apply_swipe(&mut st, 300, 400, 100, 400, 1000);
        assert_eq!(st.home_page, HOME_PAGES - 1);
    }

    #[test]
    fn swipe_right_clamps_at_zero() {
        let mut st = dev();
        apply_swipe(&mut st, 100, 400, 300, 400, 1000);
        assert_eq!(st.home_page, 0);
    }

    #[test]
    fn subthreshold_swipe_is_noop() {
        let mut st = dev();
        apply_swipe(&mut st, 100, 400, 120, 400, 1000); // dx=20 < 40
        assert_eq!(st.home_page, 0);
    }

    #[test]
    fn swipe_up_scrolls_app_clamped() {
        let mut st = dev();
        st.screen = Screen::App { index: 0 };
        apply_swipe(&mut st, 195, 700, 195, 100, 1000); // dy=-600 → scroll +600
        assert_eq!(st.app_scroll, APP_MAX_SCROLL);
    }

    #[test]
    fn swipe_down_clamps_scroll_at_zero() {
        let mut st = dev();
        st.screen = Screen::App { index: 0 };
        st.app_scroll = 100;
        apply_swipe(&mut st, 195, 100, 195, 700, 1000); // dy=+600 → scroll -600
        assert_eq!(st.app_scroll, 0);
    }

    #[test]
    fn long_press_records_flash() {
        let mut st = dev();
        apply_long_press(&mut st, 50, 60, 5000);
        assert_eq!(st.last_input_kind, InputKind::LongPress);
        assert_eq!((st.last_input_x, st.last_input_y), (50, 60));
        assert_eq!(st.last_input_at_us, 5000);
    }

    #[test]
    fn snapshot_mirrors_state() {
        let mut st = dev();
        let (x, y) = icon_center(1, 1);
        apply_tap(&mut st, x, y, 1234);
        let snap = st.snapshot();
        assert_eq!(snap.screen, st.screen);
        assert_eq!(snap.interactions, 1);
        assert_eq!(snap.last_input_at_us, 1234);
    }
}
