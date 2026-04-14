/// Game mode: low-latency optimizations for interactive use.
///
/// When enabled:
///   - Disables all overlays (stats, touch, annotations)
///   - Reduces decode buffer to 1 frame (no buffering)
///   - Disables recording/notification capture to free CPU
///   - Prioritizes display thread scheduling

#[derive(Clone, Debug)]
pub struct GameMode {
    pub enabled: bool,
    /// Saved overlay states to restore when game mode is disabled.
    saved_stats_overlay: bool,
    saved_touch_overlay: bool,
    saved_notifications: bool,
}

impl GameMode {
    pub fn new() -> Self {
        Self {
            enabled: false,
            saved_stats_overlay: true,
            saved_touch_overlay: true,
            saved_notifications: true,
        }
    }

    /// Toggle game mode on/off.
    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        tracing::info!(enabled = self.enabled, "Game mode toggled");
        self.enabled
    }

    /// Returns true if the given feature should be suppressed.
    pub fn suppress_stats(&self) -> bool { self.enabled }
    pub fn suppress_touch_overlay(&self) -> bool { self.enabled }
    pub fn suppress_notifications(&self) -> bool { self.enabled }
    pub fn suppress_recording(&self) -> bool { self.enabled }

    /// Target frame buffer size (1 = minimum latency).
    pub fn frame_buffer_size(&self) -> usize {
        if self.enabled { 1 } else { 3 }
    }
}
