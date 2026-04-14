use serde::{Deserialize, Serialize};

/// Custom themes: color schemes for the display window and overlays.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub background: [u8; 3],
    pub foreground: [u8; 3],
    pub accent: [u8; 3],
    pub overlay_bg: [u8; 4],    // RGBA
    pub overlay_text: [u8; 3],
    pub border: [u8; 3],
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            name: "Dark".into(),
            background: [0x1A, 0x1A, 0x2E],
            foreground: [0xEE, 0xEE, 0xEE],
            accent: [0x00, 0xD4, 0xFF],
            overlay_bg: [0x00, 0x00, 0x00, 0x80],
            overlay_text: [0x00, 0xD4, 0xFF],
            border: [0x33, 0x33, 0x55],
        }
    }

    pub fn light() -> Self {
        Self {
            name: "Light".into(),
            background: [0xF5, 0xF5, 0xF5],
            foreground: [0x22, 0x22, 0x22],
            accent: [0x00, 0x7A, 0xFF],
            overlay_bg: [0xFF, 0xFF, 0xFF, 0xC0],
            overlay_text: [0x00, 0x7A, 0xFF],
            border: [0xCC, 0xCC, 0xCC],
        }
    }

    pub fn midnight() -> Self {
        Self {
            name: "Midnight".into(),
            background: [0x0A, 0x0A, 0x1A],
            foreground: [0xCC, 0xCC, 0xCC],
            accent: [0xFF, 0x44, 0x88],
            overlay_bg: [0x0A, 0x0A, 0x1A, 0xC0],
            overlay_text: [0xFF, 0x44, 0x88],
            border: [0x22, 0x22, 0x44],
        }
    }

    pub fn nature() -> Self {
        Self {
            name: "Nature".into(),
            background: [0x1A, 0x2E, 0x1A],
            foreground: [0xDD, 0xEE, 0xDD],
            accent: [0x44, 0xFF, 0x88],
            overlay_bg: [0x1A, 0x2E, 0x1A, 0xC0],
            overlay_text: [0x44, 0xFF, 0x88],
            border: [0x33, 0x55, 0x33],
        }
    }

    pub fn all_themes() -> Vec<Theme> {
        vec![Self::dark(), Self::light(), Self::midnight(), Self::nature()]
    }
}

/// Get background color as u32 for minifb.
pub fn bg_color_u32(theme: &Theme) -> u32 {
    let [r, g, b] = theme.background;
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}
