//! Shared display-window state mutated by dispatch handlers and read by
//! the render loop. Pulled out so REST / Stream Deck / hotkeys can flip
//! the same flags as the display window's own keyboard handler.
//!
//! The display loop locks this once per frame; mutations from dispatch
//! handlers are short and don't span awaits, so a `std::sync::Mutex` is
//! sufficient — no need for Tokio's async mutex.

use crate::features::annotation::AnnotationLayer;
use crate::features::game_mode::GameMode;
use crate::features::zoom::ZoomState;

#[derive(Debug, Clone)]
pub struct DisplayState {
    pub zoom: ZoomState,
    pub game_mode: GameMode,
    pub annotations: AnnotationLayer,
    /// Whether the stats overlay should render. Toggled by `stats_toggle`.
    pub stats_visible: bool,
}

impl DisplayState {
    pub fn new() -> Self {
        Self {
            zoom: ZoomState::new(),
            game_mode: GameMode::new(),
            annotations: AnnotationLayer::new(),
            stats_visible: false,
        }
    }
}

impl Default for DisplayState {
    fn default() -> Self {
        Self::new()
    }
}
