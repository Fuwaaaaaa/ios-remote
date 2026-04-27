use serde::Serialize;
use std::sync::OnceLock;

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
        Command {
            id: "screenshot",
            name: "Take Screenshot",
            shortcut: Some("S"),
            category: "Capture",
        },
        Command {
            id: "screenshot_clipboard",
            name: "Screenshot to Clipboard",
            shortcut: Some("Ctrl+C"),
            category: "Capture",
        },
        Command {
            id: "record_start",
            name: "Start Recording",
            shortcut: Some("F2"),
            category: "Capture",
        },
        Command {
            id: "record_stop",
            name: "Stop Recording",
            shortcut: Some("F2"),
            category: "Capture",
        },
        Command {
            id: "gif_save",
            name: "Save GIF (last 5s)",
            shortcut: Some("G"),
            category: "Capture",
        },
        Command {
            id: "ocr",
            name: "Extract Text (OCR)",
            shortcut: Some("F3"),
            category: "Analysis",
        },
        Command {
            id: "ocr_clipboard",
            name: "OCR → Clipboard",
            shortcut: Some("Ctrl+T"),
            category: "Analysis",
        },
        Command {
            id: "ai_describe",
            name: "AI Describe Screen",
            shortcut: None,
            category: "Analysis",
        },
        Command {
            id: "qr_scan",
            name: "Scan QR Code",
            shortcut: None,
            category: "Analysis",
        },
        Command {
            id: "color_pick",
            name: "Color Picker",
            shortcut: Some("I"),
            category: "Analysis",
        },
        Command {
            id: "zoom_in",
            name: "Zoom In",
            shortcut: Some("Scroll Up"),
            category: "View",
        },
        Command {
            id: "zoom_out",
            name: "Zoom Out",
            shortcut: Some("Scroll Down"),
            category: "View",
        },
        Command {
            id: "zoom_reset",
            name: "Reset Zoom",
            shortcut: Some("R"),
            category: "View",
        },
        Command {
            id: "pip_toggle",
            name: "Toggle PiP Mode",
            shortcut: Some("P"),
            category: "View",
        },
        Command {
            id: "game_mode",
            name: "Toggle Game Mode",
            shortcut: Some("F5"),
            category: "View",
        },
        Command {
            id: "stats_toggle",
            name: "Toggle Stats Overlay",
            shortcut: Some("F4"),
            category: "View",
        },
        Command {
            id: "annotation_rect",
            name: "Draw Rectangle",
            shortcut: None,
            category: "Annotate",
        },
        Command {
            id: "annotation_arrow",
            name: "Draw Arrow",
            shortcut: None,
            category: "Annotate",
        },
        Command {
            id: "annotation_text",
            name: "Add Text",
            shortcut: None,
            category: "Annotate",
        },
        Command {
            id: "annotation_clear",
            name: "Clear Annotations",
            shortcut: None,
            category: "Annotate",
        },
        Command {
            id: "ruler",
            name: "Measure Distance",
            shortcut: Some("M"),
            category: "Annotate",
        },
        Command {
            id: "privacy_add",
            name: "Add Privacy Zone",
            shortcut: None,
            category: "Privacy",
        },
        Command {
            id: "privacy_clear",
            name: "Clear Privacy Zones",
            shortcut: None,
            category: "Privacy",
        },
        Command {
            id: "translate",
            name: "Translate Screen",
            shortcut: None,
            category: "Tools",
        },
        Command {
            id: "macro_run",
            name: "Run Macro...",
            shortcut: None,
            category: "Tools",
        },
        Command {
            id: "lua_run",
            name: "Run Lua Script...",
            shortcut: None,
            category: "Tools",
        },
        Command {
            id: "network_diag",
            name: "Network Diagnostics",
            shortcut: None,
            category: "Tools",
        },
        Command {
            id: "settings",
            name: "Open Settings",
            shortcut: Some("Ctrl+,"),
            category: "System",
        },
        Command {
            id: "web_dashboard",
            name: "Open Web Dashboard",
            shortcut: None,
            category: "System",
        },
        Command {
            id: "check_update",
            name: "Check for Updates",
            shortcut: None,
            category: "System",
        },
        Command {
            id: "firewall_setup",
            name: "Configure Firewall",
            shortcut: None,
            category: "System",
        },
        Command {
            id: "startup_toggle",
            name: "Toggle Auto-Start",
            shortcut: None,
            category: "System",
        },
        Command {
            id: "quit",
            name: "Quit",
            shortcut: Some("Q"),
            category: "System",
        },
    ]
}

/// Cached command list. The set is fixed at compile time, so we build it
/// exactly once on first access instead of per-call (the previous
/// `Box::leak` on every `search()` call leaked ~3 KB each invocation).
fn commands_cached() -> &'static [Command] {
    static CACHE: OnceLock<Vec<Command>> = OnceLock::new();
    CACHE.get_or_init(all_commands)
}

/// Fuzzy search commands by query.
pub fn search(query: &str) -> Vec<&'static Command> {
    let query_lower = query.to_lowercase();

    commands_cached()
        .iter()
        .filter(|cmd| {
            cmd.name.to_lowercase().contains(&query_lower)
                || cmd.id.contains(&query_lower)
                || cmd.category.to_lowercase().contains(&query_lower)
        })
        .collect()
}

// ─── Dispatch ────────────────────────────────────────────────────────────────
//
// Phase A: 12 "ready" actions (no extra display-state plumbing required) are
// wired to existing handlers via `ApiState`. The rest return a structured
// error so callers (Stream Deck, REST API, hotkeys) can render a clear "not
// yet" message instead of silently doing nothing.
//
// Phases B-D will expand this:
//   B. Promote display-window state (zoom, pip, game_mode, stats, annotations)
//      to shared handles so zoom_*/pip_toggle/game_mode/stats_toggle/
//      annotation_clear become dispatchable.
//   C. Wire interactive commands (color_pick, annotation_rect/arrow/text,
//      ruler, privacy_add) once the display window pipes mouse events out.
//   D. Picker dialogs (macro_run, lua_run, network_diag, settings,
//      firewall_setup) and shell-out commands (web_dashboard).

use crate::ui::api::ApiState;

/// Errors that can arise while dispatching a command id.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("unknown action id: {0}")]
    UnknownAction(String),
    /// The action is recognized but its handler is not wired in this build
    /// phase (or requires interactive UI input the dispatch path cannot
    /// supply). The variant carries a stable reason so the caller can show a
    /// helpful hint.
    #[error("'{action}' is not dispatchable: {reason}")]
    NotDispatchable {
        action: String,
        reason: &'static str,
    },
    /// No frame has been received yet — most analysis commands need one.
    #[error("no frame available yet — connect a device first")]
    NoFrame,
    /// The handler ran but returned an error.
    #[error("'{action}' failed: {message}")]
    Failed { action: String, message: String },
}

/// Outcome of a successful dispatch.
#[derive(Debug, Clone, Serialize)]
pub struct CommandResult {
    pub action: String,
    pub message: String,
}

impl CommandResult {
    fn ok(action: &str, message: impl Into<String>) -> Self {
        Self {
            action: action.to_string(),
            message: message.into(),
        }
    }
}

/// Dispatch a command id to its handler.
///
/// Synchronous: every wired handler is itself sync (no async I/O). Network
/// calls inside handlers (ai_describe, check_update) block briefly but the
/// caller already runs them off the UI thread (Stream Deck loop, REST task).
pub fn execute(action_id: &str, state: &ApiState) -> Result<CommandResult, CommandError> {
    match action_id {
        // ── Capture ─────────────────────────────────────────────────────────
        "screenshot" => {
            let frame = state.frame_bus.latest_frame().ok_or(CommandError::NoFrame)?;
            let path = crate::features::screenshot::save_frame(&frame).map_err(|m| {
                CommandError::Failed {
                    action: "screenshot".into(),
                    message: m,
                }
            })?;
            Ok(CommandResult::ok("screenshot", format!("saved → {path}")))
        }
        "screenshot_clipboard" => {
            crate::features::clipboard_sync::copy_screenshot_to_clipboard(&state.frame_bus)
                .map_err(|m| CommandError::Failed {
                    action: "screenshot_clipboard".into(),
                    message: m,
                })?;
            Ok(CommandResult::ok(
                "screenshot_clipboard",
                "copied to clipboard",
            ))
        }
        "record_start" => {
            let path = state.recorder.start().map_err(|m| CommandError::Failed {
                action: "record_start".into(),
                message: m,
            })?;
            Ok(CommandResult::ok(
                "record_start",
                format!("recording → {}", path.display()),
            ))
        }
        "record_stop" => match state.recorder.stop() {
            Some(path) => Ok(CommandResult::ok(
                "record_stop",
                format!("saved → {}", path.display()),
            )),
            None => Err(CommandError::Failed {
                action: "record_stop".into(),
                message: "no recording in progress".into(),
            }),
        },

        // ── Analysis ────────────────────────────────────────────────────────
        "ocr" => {
            let frame = state.frame_bus.latest_frame().ok_or(CommandError::NoFrame)?;
            let text = crate::features::ocr::extract_text(&frame, None).map_err(|m| {
                CommandError::Failed {
                    action: "ocr".into(),
                    message: m,
                }
            })?;
            Ok(CommandResult::ok("ocr", text))
        }
        "ocr_clipboard" => {
            let text = crate::features::clipboard_sync::copy_screen_text_to_clipboard(
                &state.frame_bus,
            )
            .map_err(|m| CommandError::Failed {
                action: "ocr_clipboard".into(),
                message: m,
            })?;
            Ok(CommandResult::ok("ocr_clipboard", format!("copied: {text}")))
        }
        "ai_describe" => {
            let frame = state.frame_bus.latest_frame().ok_or(CommandError::NoFrame)?;
            let desc = crate::features::ai_vision::describe_screen(&frame, None).map_err(|m| {
                CommandError::Failed {
                    action: "ai_describe".into(),
                    message: m,
                }
            })?;
            Ok(CommandResult::ok("ai_describe", desc))
        }
        "qr_scan" => {
            let frame = state.frame_bus.latest_frame().ok_or(CommandError::NoFrame)?;
            let codes = crate::features::qr_scanner::scan_qr_codes(&frame);
            let msg = if codes.is_empty() {
                "no QR codes found".to_string()
            } else {
                codes.join(" | ")
            };
            Ok(CommandResult::ok("qr_scan", msg))
        }

        // ── System ──────────────────────────────────────────────────────────
        "check_update" => {
            match crate::system::updater::check_for_update().map_err(|m| CommandError::Failed {
                action: "check_update".into(),
                message: m,
            })? {
                Some(info) => Ok(CommandResult::ok(
                    "check_update",
                    format!("update available: {info:?}"),
                )),
                None => Ok(CommandResult::ok("check_update", "up to date")),
            }
        }
        "startup_toggle" => {
            if crate::system::startup::is_startup_enabled() {
                crate::system::startup::disable_startup().map_err(|m| CommandError::Failed {
                    action: "startup_toggle".into(),
                    message: m,
                })?;
                Ok(CommandResult::ok("startup_toggle", "auto-start disabled"))
            } else {
                crate::system::startup::enable_startup().map_err(|m| CommandError::Failed {
                    action: "startup_toggle".into(),
                    message: m,
                })?;
                Ok(CommandResult::ok("startup_toggle", "auto-start enabled"))
            }
        }
        "quit" => {
            tracing::info!("quit requested via command palette");
            std::process::exit(0);
        }

        // ── Not yet wired (Phase B: needs shared display-window state) ─────
        "zoom_in" | "zoom_out" | "zoom_reset" | "pip_toggle" | "game_mode" | "stats_toggle"
        | "annotation_clear" => Err(CommandError::NotDispatchable {
            action: action_id.to_string(),
            reason: "display-state actions are not wired yet (Phase B)",
        }),

        // ── Interactive (Phase C: needs mouse events from display) ──────────
        "color_pick" | "annotation_rect" | "annotation_arrow" | "annotation_text" | "ruler"
        | "privacy_add" | "privacy_clear" => Err(CommandError::NotDispatchable {
            action: action_id.to_string(),
            reason: "requires interactive input from the display window",
        }),

        // ── Launchers (Phase D, no-arg) ─────────────────────────────────────
        "web_dashboard" => {
            launch_url(&state.dashboard_url).map_err(|m| CommandError::Failed {
                action: "web_dashboard".into(),
                message: m,
            })?;
            Ok(CommandResult::ok(
                "web_dashboard",
                format!("opened {}", state.dashboard_url),
            ))
        }
        "settings" => {
            launch_path("ios-remote.toml").map_err(|m| CommandError::Failed {
                action: "settings".into(),
                message: m,
            })?;
            Ok(CommandResult::ok("settings", "opened ios-remote.toml"))
        }
        "firewall_setup" => {
            launch_path("wf.msc").map_err(|m| CommandError::Failed {
                action: "firewall_setup".into(),
                message: m,
            })?;
            Ok(CommandResult::ok(
                "firewall_setup",
                "Windows Firewall console opened",
            ))
        }
        "translate" => {
            let frame = state.frame_bus.latest_frame().ok_or(CommandError::NoFrame)?;
            let mut overlay = crate::features::translation::TranslationOverlay::new("en", "ja");
            let pairs = overlay
                .translate_frame(&frame)
                .map_err(|m| CommandError::Failed {
                    action: "translate".into(),
                    message: m,
                })?;
            let summary = if pairs.is_empty() {
                "no text detected".to_string()
            } else {
                pairs
                    .into_iter()
                    .map(|(orig, trans)| format!("{orig} → {trans}"))
                    .collect::<Vec<_>>()
                    .join(" | ")
            };
            Ok(CommandResult::ok("translate", summary))
        }

        // ── Picker-required (Phase D follow-up: needs args from caller) ─────
        "gif_save" | "macro_run" | "lua_run" | "network_diag" => {
            Err(CommandError::NotDispatchable {
                action: action_id.to_string(),
                reason: "needs caller-supplied arguments (Phase D follow-up)",
            })
        }

        unknown => Err(CommandError::UnknownAction(unknown.to_string())),
    }
}

/// Launch a URL via the Windows shell (`explorer <url>`). Falls back to
/// `cmd /C start` if the explorer call fails so users with custom
/// default-browser handlers still get a response.
fn launch_url(url: &str) -> Result<(), String> {
    use std::process::Command;
    Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .map_err(|e| format!("failed to spawn shell: {e}"))?;
    Ok(())
}

/// Open a file or `.msc` snap-in via the Windows shell. Same dispatch
/// path as `launch_url`; kept as a separate function so future tightening
/// (path canonicalization, existence check) can land in one spot.
fn launch_path(path: &str) -> Result<(), String> {
    use std::process::Command;
    Command::new("cmd")
        .args(["/C", "start", "", path])
        .spawn()
        .map_err(|e| format!("failed to spawn shell: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_state() -> ApiState {
        let bus = crate::features::FrameBus::new();
        ApiState {
            frame_bus: bus.clone(),
            config: std::sync::Arc::new(tokio::sync::Mutex::new(
                crate::config::AppConfig::default(),
            )),
            history: std::sync::Arc::new(tokio::sync::Mutex::new(
                crate::config::ConnectionHistory::default(),
            )),
            stats: std::sync::Arc::new(tokio::sync::Mutex::new(
                crate::ui::api::StreamStats::default(),
            )),
            api_token: String::new(),
            recorder: crate::features::recording::RecordingController::new(bus.clone()),
            replay: crate::features::session_replay::SessionPlaybackController::new(bus),
            dashboard_url: "http://127.0.0.1:8080".into(),
        }
    }

    #[test]
    fn search_finds_by_id_name_and_category() {
        let by_id = search("screenshot");
        assert!(by_id.iter().any(|c| c.id == "screenshot"));
        let by_category = search("Capture");
        assert!(by_category.iter().any(|c| c.category == "Capture"));
        let by_name = search("Take Screenshot");
        assert!(by_name.iter().any(|c| c.id == "screenshot"));
    }

    #[test]
    fn cached_command_list_is_stable() {
        let a = commands_cached();
        let b = commands_cached();
        assert!(std::ptr::eq(a, b), "OnceLock must return the same slice");
        assert_eq!(a.len(), all_commands().len());
    }

    #[test]
    fn unknown_action_returns_unknown_error() {
        let state = dummy_state();
        let err = execute("not_a_real_action", &state).expect_err("should be unknown");
        match err {
            CommandError::UnknownAction(id) => assert_eq!(id, "not_a_real_action"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn phase_b_and_c_actions_still_report_not_dispatchable() {
        let state = dummy_state();
        // zoom_in is Phase B, color_pick is Phase C — both stay 409 until
        // their respective follow-up PRs land.
        for id in ["zoom_in", "color_pick"] {
            let err = execute(id, &state).expect_err("should not be dispatchable yet");
            assert!(
                matches!(err, CommandError::NotDispatchable { .. }),
                "{id} should be NotDispatchable, got {err:?}"
            );
        }
    }
}
