/// Screen rotation detection: detect iPhone orientation changes.
///
/// When the H.264 stream changes aspect ratio (e.g., 1920x1080 → 1080x1920),
/// we detect the rotation and can auto-adjust the display window.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orientation {
    Portrait,
    Landscape,
    PortraitUpsideDown,
    LandscapeRight,
}

pub struct RotationDetector {
    current: Orientation,
    last_width: u32,
    last_height: u32,
}

impl RotationDetector {
    pub fn new() -> Self {
        Self {
            current: Orientation::Portrait,
            last_width: 0,
            last_height: 0,
        }
    }

    /// Update with new frame dimensions. Returns Some(new_orientation) if changed.
    pub fn update(&mut self, width: u32, height: u32) -> Option<Orientation> {
        if width == self.last_width && height == self.last_height {
            return None;
        }
        self.last_width = width;
        self.last_height = height;

        let new = if width > height {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        };
        if new != self.current {
            let old = self.current;
            self.current = new;
            tracing::info!(from = ?old, to = ?new, "Screen rotation detected");
            Some(new)
        } else {
            None
        }
    }

    pub fn current(&self) -> Orientation {
        self.current
    }
}

/// Mirror flip: horizontal or vertical flip of the frame.
pub fn flip_horizontal(rgba: &mut [u8], width: u32, height: u32) {
    let w = width as usize;
    for y in 0..height as usize {
        for x in 0..w / 2 {
            let left = (y * w + x) * 4;
            let right = (y * w + (w - 1 - x)) * 4;
            for c in 0..4 {
                rgba.swap(left + c, right + c);
            }
        }
    }
}

pub fn flip_vertical(rgba: &mut [u8], width: u32, height: u32) {
    let w = width as usize;
    let row_bytes = w * 4;
    let h = height as usize;
    for y in 0..h / 2 {
        let top = y * row_bytes;
        let bot = (h - 1 - y) * row_bytes;
        for i in 0..row_bytes {
            rgba.swap(top + i, bot + i);
        }
    }
}

/// Crop region presets: save/load named crop regions.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CropPreset {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub struct CropPresets {
    pub presets: Vec<CropPreset>,
}

impl CropPresets {
    pub fn new() -> Self {
        Self {
            presets: Vec::new(),
        }
    }

    pub fn add(&mut self, name: &str, x: u32, y: u32, w: u32, h: u32) {
        self.presets.push(CropPreset {
            name: name.to_string(),
            x,
            y,
            w,
            h,
        });
    }

    pub fn get(&self, name: &str) -> Option<&CropPreset> {
        self.presets.iter().find(|p| p.name == name)
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.presets).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn load(path: &str) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let presets = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        Ok(Self { presets })
    }
}
