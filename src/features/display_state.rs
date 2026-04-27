//! Shared display-window state mutated by dispatch handlers and read by
//! the render loop. Pulled out so REST / Stream Deck / hotkeys can flip
//! the same flags as the display window's own keyboard handler.
//!
//! The display loop locks this once per frame; mutations from dispatch
//! handlers are short and don't span awaits, so a `std::sync::Mutex` is
//! sufficient — no need for Tokio's async mutex.

use crate::features::annotation::AnnotationLayer;
use crate::features::color_picker::PickedColor;
use crate::features::game_mode::GameMode;
use crate::features::zoom::ZoomState;

/// Interactive command waiting on the next click in the display window.
/// Phase C dispatch sets this; the display mouse handler completes it.
/// More variants will land as the rest of Phase C wires up
/// (annotation_rect, ruler, privacy_add) — they all share this slot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PendingInteractive {
    #[default]
    Idle,
    /// Next left click reads RGBA at the cursor and stores the result in
    /// `DisplayState::last_picked`.
    ColorPick,
}

#[derive(Debug, Clone)]
pub struct DisplayState {
    pub zoom: ZoomState,
    pub game_mode: GameMode,
    pub annotations: AnnotationLayer,
    /// Whether the stats overlay should render. Toggled by `stats_toggle`.
    pub stats_visible: bool,
    /// What the next display-window click should do. Defaults to `Idle`.
    pub pending: PendingInteractive,
    /// Result of the most recent `color_pick`. Cleared by `color_pick`
    /// dispatch when a new pick is requested.
    pub last_picked: Option<PickedColor>,
}

impl DisplayState {
    pub fn new() -> Self {
        Self {
            zoom: ZoomState::new(),
            game_mode: GameMode::new(),
            annotations: AnnotationLayer::new(),
            stats_visible: false,
            pending: PendingInteractive::Idle,
            last_picked: None,
        }
    }
}

impl Default for DisplayState {
    fn default() -> Self {
        Self::new()
    }
}
