use tracing::{info, warn};
use winreg::RegKey;
use winreg::enums::*;

const APP_NAME: &str = "ios-remote";

/// Register ios-remote to start on Windows login.
pub fn enable_startup() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE,
        )
        .map_err(|e| format!("Registry open failed: {}", e))?;

    let exe_path = std::env::current_exe().map_err(|e| format!("Failed to get exe path: {}", e))?;

    run_key
        .set_value(APP_NAME, &exe_path.to_string_lossy().as_ref())
        .map_err(|e| format!("Registry write failed: {}", e))?;

    info!(path = %exe_path.display(), "Startup registration: enabled");
    Ok(())
}

/// Remove ios-remote from Windows startup.
pub fn disable_startup() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE,
        )
        .map_err(|e| format!("Registry open failed: {}", e))?;

    match run_key.delete_value(APP_NAME) {
        Ok(_) => {
            info!("Startup registration: disabled");
            Ok(())
        }
        Err(e) => {
            warn!(error = %e, "Startup entry not found or already removed");
            Ok(())
        }
    }
}

/// Check if startup is currently enabled.
pub fn is_startup_enabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run_key) = hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") {
        run_key.get_value::<String, _>(APP_NAME).is_ok()
    } else {
        false
    }
}

/// Show iPhone battery level in taskbar (via tray icon tooltip).
pub struct BatteryWidget {
    pub level: u8,
    pub charging: bool,
}

impl BatteryWidget {
    pub fn new() -> Self {
        Self {
            level: 0,
            charging: false,
        }
    }

    pub fn update(&mut self, level: u8, charging: bool) {
        self.level = level;
        self.charging = charging;
    }

    pub fn tooltip(&self) -> String {
        let icon = if self.charging { "⚡" } else { "🔋" };
        format!("iPhone {} {}%", icon, self.level)
    }
}
