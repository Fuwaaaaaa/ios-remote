use serde::{Deserialize, Serialize};

/// Stream Deck integration: map Elgato Stream Deck buttons to actions.
///
/// Connects via Stream Deck SDK WebSocket (localhost:28196).

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamDeckButton {
    pub position: u8,
    pub action: String, // command palette command ID
    pub label: String,
    pub icon: Option<String>, // base64 PNG
}

pub struct StreamDeckIntegration {
    buttons: Vec<StreamDeckButton>,
    connected: bool,
}

impl StreamDeckIntegration {
    pub fn new() -> Self {
        Self { buttons: Self::default_layout(), connected: false }
    }

    fn default_layout() -> Vec<StreamDeckButton> {
        vec![
            StreamDeckButton { position: 0, action: "screenshot".into(), label: "Screenshot".into(), icon: None },
            StreamDeckButton { position: 1, action: "record_start".into(), label: "Record".into(), icon: None },
            StreamDeckButton { position: 2, action: "record_stop".into(), label: "Stop".into(), icon: None },
            StreamDeckButton { position: 3, action: "ocr".into(), label: "OCR".into(), icon: None },
            StreamDeckButton { position: 4, action: "gif_save".into(), label: "GIF".into(), icon: None },
            StreamDeckButton { position: 5, action: "pip_toggle".into(), label: "PiP".into(), icon: None },
            StreamDeckButton { position: 6, action: "game_mode".into(), label: "Game".into(), icon: None },
            StreamDeckButton { position: 7, action: "ai_describe".into(), label: "AI".into(), icon: None },
        ]
    }

    /// Handle a button press by position.
    pub fn on_press(&self, position: u8) -> Option<&str> {
        self.buttons.iter()
            .find(|b| b.position == position)
            .map(|b| b.action.as_str())
    }

    pub fn save_layout(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.buttons).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn load_layout(&mut self, path: &str) -> Result<(), String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        self.buttons = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        Ok(())
    }
}
