use serde::{Deserialize, Serialize};

/// Gesture library: pre-built multi-touch gesture presets.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gesture {
    pub name: String,
    pub points: Vec<GesturePoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GesturePoint {
    pub x: f32,  // 0.0-1.0 relative to screen
    pub y: f32,
    pub time_ms: u64,
    pub finger: u8, // finger index for multi-touch
    pub action: TouchAction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TouchAction { Down, Move, Up }

/// Built-in gesture presets.
pub fn preset_gestures() -> Vec<Gesture> {
    vec![
        Gesture {
            name: "Swipe Up".to_string(),
            points: vec![
                GesturePoint { x: 0.5, y: 0.7, time_ms: 0, finger: 0, action: TouchAction::Down },
                GesturePoint { x: 0.5, y: 0.3, time_ms: 300, finger: 0, action: TouchAction::Move },
                GesturePoint { x: 0.5, y: 0.3, time_ms: 310, finger: 0, action: TouchAction::Up },
            ],
        },
        Gesture {
            name: "Swipe Down".to_string(),
            points: vec![
                GesturePoint { x: 0.5, y: 0.3, time_ms: 0, finger: 0, action: TouchAction::Down },
                GesturePoint { x: 0.5, y: 0.7, time_ms: 300, finger: 0, action: TouchAction::Move },
                GesturePoint { x: 0.5, y: 0.7, time_ms: 310, finger: 0, action: TouchAction::Up },
            ],
        },
        Gesture {
            name: "Pinch In".to_string(),
            points: vec![
                GesturePoint { x: 0.3, y: 0.5, time_ms: 0, finger: 0, action: TouchAction::Down },
                GesturePoint { x: 0.7, y: 0.5, time_ms: 0, finger: 1, action: TouchAction::Down },
                GesturePoint { x: 0.45, y: 0.5, time_ms: 300, finger: 0, action: TouchAction::Move },
                GesturePoint { x: 0.55, y: 0.5, time_ms: 300, finger: 1, action: TouchAction::Move },
                GesturePoint { x: 0.45, y: 0.5, time_ms: 310, finger: 0, action: TouchAction::Up },
                GesturePoint { x: 0.55, y: 0.5, time_ms: 310, finger: 1, action: TouchAction::Up },
            ],
        },
        Gesture {
            name: "Pinch Out".to_string(),
            points: vec![
                GesturePoint { x: 0.45, y: 0.5, time_ms: 0, finger: 0, action: TouchAction::Down },
                GesturePoint { x: 0.55, y: 0.5, time_ms: 0, finger: 1, action: TouchAction::Down },
                GesturePoint { x: 0.3, y: 0.5, time_ms: 300, finger: 0, action: TouchAction::Move },
                GesturePoint { x: 0.7, y: 0.5, time_ms: 300, finger: 1, action: TouchAction::Move },
                GesturePoint { x: 0.3, y: 0.5, time_ms: 310, finger: 0, action: TouchAction::Up },
                GesturePoint { x: 0.7, y: 0.5, time_ms: 310, finger: 1, action: TouchAction::Up },
            ],
        },
        Gesture {
            name: "3-Finger Swipe Up (App Switcher)".to_string(),
            points: vec![
                GesturePoint { x: 0.3, y: 0.8, time_ms: 0, finger: 0, action: TouchAction::Down },
                GesturePoint { x: 0.5, y: 0.8, time_ms: 0, finger: 1, action: TouchAction::Down },
                GesturePoint { x: 0.7, y: 0.8, time_ms: 0, finger: 2, action: TouchAction::Down },
                GesturePoint { x: 0.3, y: 0.4, time_ms: 300, finger: 0, action: TouchAction::Move },
                GesturePoint { x: 0.5, y: 0.4, time_ms: 300, finger: 1, action: TouchAction::Move },
                GesturePoint { x: 0.7, y: 0.4, time_ms: 300, finger: 2, action: TouchAction::Move },
                GesturePoint { x: 0.3, y: 0.4, time_ms: 310, finger: 0, action: TouchAction::Up },
                GesturePoint { x: 0.5, y: 0.4, time_ms: 310, finger: 1, action: TouchAction::Up },
                GesturePoint { x: 0.7, y: 0.4, time_ms: 310, finger: 2, action: TouchAction::Up },
            ],
        },
        Gesture {
            name: "Rotate CW".to_string(),
            points: vec![
                GesturePoint { x: 0.4, y: 0.4, time_ms: 0, finger: 0, action: TouchAction::Down },
                GesturePoint { x: 0.6, y: 0.6, time_ms: 0, finger: 1, action: TouchAction::Down },
                GesturePoint { x: 0.6, y: 0.4, time_ms: 300, finger: 0, action: TouchAction::Move },
                GesturePoint { x: 0.4, y: 0.6, time_ms: 300, finger: 1, action: TouchAction::Move },
                GesturePoint { x: 0.6, y: 0.4, time_ms: 310, finger: 0, action: TouchAction::Up },
                GesturePoint { x: 0.4, y: 0.6, time_ms: 310, finger: 1, action: TouchAction::Up },
            ],
        },
    ]
}
