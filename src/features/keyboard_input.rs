use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;

/// Keyboard pass-through: send PC keyboard input to iPhone.
///
/// Maps PC key events to iPhone text input. Requires an active input
/// channel (WebDriverAgent, companion app, or Bluetooth HID).

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyMapping {
    pub mappings: HashMap<String, KeyAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum KeyAction {
    /// Type a character
    Char(char),
    /// Special key (Return, Backspace, Tab, etc.)
    Special(SpecialKey),
    /// Trigger a macro
    RunMacro(String),
    /// Take a screenshot
    Screenshot,
    /// Toggle recording
    ToggleRecord,
    /// Run OCR
    RunOcr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SpecialKey {
    Return,
    Backspace,
    Tab,
    Escape,
    Home,
    VolumeUp,
    VolumeDown,
    Lock,
}

impl KeyMapping {
    pub fn default_mapping() -> Self {
        let mut m = HashMap::new();

        // Function key shortcuts
        m.insert("F1".to_string(), KeyAction::Screenshot);
        m.insert("F2".to_string(), KeyAction::ToggleRecord);
        m.insert("F3".to_string(), KeyAction::RunOcr);

        // Special keys
        m.insert("Return".to_string(), KeyAction::Special(SpecialKey::Return));
        m.insert(
            "Backspace".to_string(),
            KeyAction::Special(SpecialKey::Backspace),
        );
        m.insert("Tab".to_string(), KeyAction::Special(SpecialKey::Tab));

        Self { mappings: m }
    }

    /// Process a key press and return the action to execute.
    pub fn handle_key(&self, key_name: &str) -> Option<&KeyAction> {
        self.mappings.get(key_name)
    }

    /// Load custom key mappings from JSON.
    pub fn load(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }

    /// Save key mappings to JSON.
    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }
}

/// Hotkey manager: tracks key combinations and triggers actions.
pub struct HotkeyManager {
    mapping: KeyMapping,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            mapping: KeyMapping::default_mapping(),
        }
    }

    pub fn with_mapping(mapping: KeyMapping) -> Self {
        Self { mapping }
    }

    pub fn process_key(&self, key: &str) -> Option<&KeyAction> {
        let action = self.mapping.handle_key(key);
        if let Some(_a) = action {
            info!(key = %key, "Hotkey triggered");
        }
        action
    }
}
