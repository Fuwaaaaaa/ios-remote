use super::wda_client::{WdaClient, default_wda_client};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// Automation macro system: record and replay sequences of actions.
///
/// A macro is a series of timed actions (tap, swipe, wait, screenshot)
/// that can be saved to JSON and replayed. Useful for repetitive tasks
/// like game farming, form filling, or testing flows.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub description: String,
    pub actions: Vec<MacroAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MacroAction {
    /// Tap at screen coordinates
    Tap { x: u32, y: u32, delay_ms: u64 },
    /// Swipe from (x1,y1) to (x2,y2) over duration_ms
    Swipe {
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u64,
        delay_ms: u64,
    },
    /// Long press at coordinates for duration_ms
    LongPress {
        x: u32,
        y: u32,
        duration_ms: u64,
        delay_ms: u64,
    },
    /// Wait before next action
    Wait { duration_ms: u64 },
    /// Take a screenshot
    Screenshot { delay_ms: u64 },
    /// Wait until screen matches a pattern (template matching)
    WaitForScreen {
        template_path: String,
        timeout_ms: u64,
        region: Option<(u32, u32, u32, u32)>,
    },
    /// Repeat previous N actions
    Repeat { count: u32, actions_back: u32 },
}

impl Macro {
    /// Load a macro from a JSON file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }

    /// Save a macro to a JSON file.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Execute the macro against a WebDriverAgent endpoint. `client` is the
    /// WDA client to use; pass `default_wda_client()` to pick up
    /// `IOS_REMOTE_WDA_URL` or the 127.0.0.1:8100 default.
    ///
    /// Actions that do not require device input (Wait, WaitForScreen,
    /// Screenshot, Repeat) run even if WDA is unreachable; input actions
    /// bubble the WDA error up to the caller.
    pub async fn execute(&self) -> Result<(), String> {
        self.execute_with(&default_wda_client()).await
    }

    pub async fn execute_with(&self, client: &WdaClient) -> Result<(), String> {
        info!(name = %self.name, actions = self.actions.len(), "Executing macro");

        for (i, action) in self.actions.iter().enumerate() {
            match action {
                MacroAction::Tap { x, y, delay_ms } => {
                    tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
                    info!(step = i, x, y, "Macro: tap");
                    client.tap(*x, *y).map_err(|e| e.to_string())?;
                }
                MacroAction::Swipe {
                    x1,
                    y1,
                    x2,
                    y2,
                    duration_ms,
                    delay_ms,
                } => {
                    tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
                    info!(step = i, "Macro: swipe ({},{})→({},{})", x1, y1, x2, y2);
                    client
                        .swipe(*x1, *y1, *x2, *y2, *duration_ms)
                        .map_err(|e| e.to_string())?;
                }
                MacroAction::LongPress {
                    x,
                    y,
                    duration_ms,
                    delay_ms,
                } => {
                    tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
                    info!(step = i, x, y, duration_ms, "Macro: long press");
                    client
                        .long_press(*x, *y, *duration_ms)
                        .map_err(|e| e.to_string())?;
                }
                MacroAction::Wait { duration_ms } => {
                    info!(step = i, duration_ms, "Macro: wait");
                    tokio::time::sleep(std::time::Duration::from_millis(*duration_ms)).await;
                }
                MacroAction::Screenshot { delay_ms } => {
                    tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
                    info!(
                        step = i,
                        "Macro: screenshot (delegated to screenshot feature)"
                    );
                    // Actual frame grab is owned by screenshot::save_frame via
                    // the API layer; here we just mark the intent so replays
                    // can time screenshots relative to input.
                }
                MacroAction::WaitForScreen {
                    template_path,
                    timeout_ms: _,
                    region: _,
                } => {
                    warn!(step = i, template = %template_path, "Macro: WaitForScreen not yet implemented — skipping");
                }
                MacroAction::Repeat {
                    count,
                    actions_back: _,
                } => {
                    warn!(
                        step = i,
                        count, "Macro: Repeat not yet implemented — skipping"
                    );
                }
            }
        }

        info!(name = %self.name, "Macro completed");
        Ok(())
    }
}

/// List saved macros from ./macros/ directory.
pub fn list_macros() -> Vec<String> {
    let dir = Path::new("macros");
    if !dir.exists() {
        return vec![];
    }

    let entries = match fs::read_dir(dir) {
        Ok(it) => it,
        Err(e) => {
            tracing::warn!(error = %e, dir = %dir.display(), "list_macros: read_dir failed");
            return vec![];
        }
    };

    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? == "json" {
                Some(path.file_stem()?.to_str()?.to_string())
            } else {
                None
            }
        })
        .collect()
}
