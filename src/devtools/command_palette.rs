use serde::Serialize;

/// Command palette: Ctrl+P style fuzzy search for all commands.
///
/// Provides a unified interface to all features via text search.

#[derive(Debug, Clone, Serialize)]
pub struct Command {
    pub id: &'static str,
    pub name: &'static str,
    pub shortcut: Option<&'static str>,
    pub category: &'static str,
}

/// All available commands.
pub fn all_commands() -> Vec<Command> {
    vec![
        Command { id: "screenshot", name: "Take Screenshot", shortcut: Some("S"), category: "Capture" },
        Command { id: "screenshot_clipboard", name: "Screenshot to Clipboard", shortcut: Some("Ctrl+C"), category: "Capture" },
        Command { id: "record_start", name: "Start Recording", shortcut: Some("F2"), category: "Capture" },
        Command { id: "record_stop", name: "Stop Recording", shortcut: Some("F2"), category: "Capture" },
        Command { id: "gif_save", name: "Save GIF (last 5s)", shortcut: Some("G"), category: "Capture" },

        Command { id: "ocr", name: "Extract Text (OCR)", shortcut: Some("F3"), category: "Analysis" },
        Command { id: "ocr_clipboard", name: "OCR → Clipboard", shortcut: Some("Ctrl+T"), category: "Analysis" },
        Command { id: "ai_describe", name: "AI Describe Screen", shortcut: None, category: "Analysis" },
        Command { id: "qr_scan", name: "Scan QR Code", shortcut: None, category: "Analysis" },
        Command { id: "color_pick", name: "Color Picker", shortcut: Some("I"), category: "Analysis" },

        Command { id: "zoom_in", name: "Zoom In", shortcut: Some("Scroll Up"), category: "View" },
        Command { id: "zoom_out", name: "Zoom Out", shortcut: Some("Scroll Down"), category: "View" },
        Command { id: "zoom_reset", name: "Reset Zoom", shortcut: Some("R"), category: "View" },
        Command { id: "pip_toggle", name: "Toggle PiP Mode", shortcut: Some("P"), category: "View" },
        Command { id: "game_mode", name: "Toggle Game Mode", shortcut: Some("F5"), category: "View" },
        Command { id: "stats_toggle", name: "Toggle Stats Overlay", shortcut: Some("F4"), category: "View" },

        Command { id: "annotation_rect", name: "Draw Rectangle", shortcut: None, category: "Annotate" },
        Command { id: "annotation_arrow", name: "Draw Arrow", shortcut: None, category: "Annotate" },
        Command { id: "annotation_text", name: "Add Text", shortcut: None, category: "Annotate" },
        Command { id: "annotation_clear", name: "Clear Annotations", shortcut: None, category: "Annotate" },
        Command { id: "ruler", name: "Measure Distance", shortcut: Some("M"), category: "Annotate" },

        Command { id: "privacy_add", name: "Add Privacy Zone", shortcut: None, category: "Privacy" },
        Command { id: "privacy_clear", name: "Clear Privacy Zones", shortcut: None, category: "Privacy" },

        Command { id: "translate", name: "Translate Screen", shortcut: None, category: "Tools" },
        Command { id: "macro_run", name: "Run Macro...", shortcut: None, category: "Tools" },
        Command { id: "lua_run", name: "Run Lua Script...", shortcut: None, category: "Tools" },
        Command { id: "network_diag", name: "Network Diagnostics", shortcut: None, category: "Tools" },

        Command { id: "settings", name: "Open Settings", shortcut: Some("Ctrl+,"), category: "System" },
        Command { id: "web_dashboard", name: "Open Web Dashboard", shortcut: None, category: "System" },
        Command { id: "check_update", name: "Check for Updates", shortcut: None, category: "System" },
        Command { id: "firewall_setup", name: "Configure Firewall", shortcut: None, category: "System" },
        Command { id: "startup_toggle", name: "Toggle Auto-Start", shortcut: None, category: "System" },
        Command { id: "quit", name: "Quit", shortcut: Some("Q"), category: "System" },
    ]
}

/// Fuzzy search commands by query.
pub fn search(query: &str) -> Vec<&'static Command> {
    let commands = all_commands();
    let query_lower = query.to_lowercase();

    // Leak to get 'static lifetime (command list is fixed)
    let commands: &'static Vec<Command> = Box::leak(Box::new(commands));

    commands
        .iter()
        .filter(|cmd| {
            cmd.name.to_lowercase().contains(&query_lower)
                || cmd.id.contains(&query_lower)
                || cmd.category.to_lowercase().contains(&query_lower)
        })
        .collect()
}
