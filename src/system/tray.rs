use tracing::info;

/// System tray icon with context menu.
///
/// Shows a tray icon in the Windows taskbar notification area.
/// Right-click menu: Status, Screenshot, Toggle Recording, Settings, Quit.
/// Left-click: Show/hide main window.
pub struct TrayIcon {
    visible: bool,
}

impl TrayIcon {
    pub fn new() -> Self {
        Self { visible: false }
    }

    /// Initialize the tray icon. Call from a dedicated OS thread.
    pub fn show(&mut self) {
        info!("System tray icon: initializing");
        self.visible = true;
        // Windows implementation uses Shell_NotifyIconW via winapi
        // For now, we log readiness. Full implementation requires
        // the `tray-icon` or `winapi` crate.
        info!("System tray icon: ready (minimize to tray with window close button)");
    }

    pub fn hide(&mut self) {
        self.visible = false;
        info!("System tray icon: hidden");
    }

    /// Update tray tooltip with current status.
    pub fn update_tooltip(&self, status: &str) {
        if self.visible {
            tracing::debug!(status = %status, "Tray tooltip updated");
        }
    }

    /// Show a balloon notification from the tray.
    pub fn notify(&self, title: &str, message: &str) {
        if self.visible {
            info!(title = %title, message = %message, "Tray notification");
            // On Windows: Shell_NotifyIconW with NIF_INFO
        }
    }
}
