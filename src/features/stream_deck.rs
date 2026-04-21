use serde::{Deserialize, Serialize};

/// Stream Deck integration: map Elgato Stream Deck buttons to actions.
///
/// The earlier draft referenced a "Stream Deck SDK WebSocket on localhost:28196".
/// That socket is only exposed to *plugins running inside the Stream Deck app*;
/// third-party binaries cannot connect to it. We instead talk to the device
/// over HID using the `elgato-streamdeck` crate, guarded by the `stream_deck`
/// cargo feature so users who don't have the hardware avoid the dependency.

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
        Self {
            buttons: Self::default_layout(),
            connected: false,
        }
    }

    fn default_layout() -> Vec<StreamDeckButton> {
        vec![
            StreamDeckButton {
                position: 0,
                action: "screenshot".into(),
                label: "Screenshot".into(),
                icon: None,
            },
            StreamDeckButton {
                position: 1,
                action: "record_start".into(),
                label: "Record".into(),
                icon: None,
            },
            StreamDeckButton {
                position: 2,
                action: "record_stop".into(),
                label: "Stop".into(),
                icon: None,
            },
            StreamDeckButton {
                position: 3,
                action: "ocr".into(),
                label: "OCR".into(),
                icon: None,
            },
            StreamDeckButton {
                position: 4,
                action: "gif_save".into(),
                label: "GIF".into(),
                icon: None,
            },
            StreamDeckButton {
                position: 5,
                action: "pip_toggle".into(),
                label: "PiP".into(),
                icon: None,
            },
            StreamDeckButton {
                position: 6,
                action: "game_mode".into(),
                label: "Game".into(),
                icon: None,
            },
            StreamDeckButton {
                position: 7,
                action: "ai_describe".into(),
                label: "AI".into(),
                icon: None,
            },
        ]
    }

    pub fn buttons(&self) -> &[StreamDeckButton] {
        &self.buttons
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Handle a button press by position.
    pub fn on_press(&self, position: u8) -> Option<&str> {
        self.buttons
            .iter()
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

/// Discover the first attached Stream Deck and return an event-producing client.
///
/// Returns `Err("stream_deck feature not enabled")` when built without the
/// `stream_deck` cargo feature, so callers can treat this as a soft-fail.
#[cfg(not(feature = "stream_deck"))]
pub fn try_open_device() -> Result<(), String> {
    Err("stream_deck feature not enabled (build with --features stream_deck)".to_string())
}

#[cfg(feature = "stream_deck")]
pub fn try_open_device() -> Result<elgato_streamdeck::StreamDeck, String> {
    use elgato_streamdeck::{StreamDeck, list_devices, new_hidapi};
    let hid = new_hidapi().map_err(|e| format!("hidapi init: {e}"))?;
    let devices = list_devices(&hid);
    let (kind, serial) = devices
        .into_iter()
        .next()
        .ok_or_else(|| "no Stream Deck attached".to_string())?;
    StreamDeck::connect(&hid, kind, &serial)
        .map_err(|e| format!("connect to Stream Deck {serial}: {e}"))
}
