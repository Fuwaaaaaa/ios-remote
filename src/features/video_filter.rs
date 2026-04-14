/// Video filters: adjust brightness, contrast, saturation, grayscale.
///
/// Applied to frames before display. All operations are in-place on RGBA.

#[derive(Clone, Debug)]
pub struct FilterSettings {
    pub brightness: f32,   // -1.0 to 1.0 (0 = normal)
    pub contrast: f32,     // 0.0 to 3.0 (1.0 = normal)
    pub saturation: f32,   // 0.0 to 3.0 (1.0 = normal, 0 = grayscale)
    pub grayscale: bool,
    pub invert: bool,
    pub sepia: bool,
}

impl Default for FilterSettings {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            grayscale: false,
            invert: false,
            sepia: false,
        }
    }
}

impl FilterSettings {
    pub fn is_default(&self) -> bool {
        (self.brightness.abs() < 0.01)
            && (self.contrast - 1.0).abs() < 0.01
            && (self.saturation - 1.0).abs() < 0.01
            && !self.grayscale
            && !self.invert
            && !self.sepia
    }
}

/// Apply filters to an RGBA buffer in-place.
pub fn apply_filters(rgba: &mut [u8], _width: u32, _height: u32, settings: &FilterSettings) {
    if settings.is_default() { return; }

    let len = rgba.len() / 4;
    for i in 0..len {
        let idx = i * 4;
        if idx + 2 >= rgba.len() { break; }

        let mut r = rgba[idx] as f32;
        let mut g = rgba[idx + 1] as f32;
        let mut b = rgba[idx + 2] as f32;

        // Brightness
        if settings.brightness.abs() > 0.01 {
            let adj = settings.brightness * 255.0;
            r += adj; g += adj; b += adj;
        }

        // Contrast
        if (settings.contrast - 1.0).abs() > 0.01 {
            r = (r - 128.0) * settings.contrast + 128.0;
            g = (g - 128.0) * settings.contrast + 128.0;
            b = (b - 128.0) * settings.contrast + 128.0;
        }

        // Saturation / Grayscale
        if settings.grayscale || (settings.saturation - 1.0).abs() > 0.01 {
            let gray = r * 0.299 + g * 0.587 + b * 0.114;
            let sat = if settings.grayscale { 0.0 } else { settings.saturation };
            r = gray + (r - gray) * sat;
            g = gray + (g - gray) * sat;
            b = gray + (b - gray) * sat;
        }

        // Invert
        if settings.invert {
            r = 255.0 - r; g = 255.0 - g; b = 255.0 - b;
        }

        // Sepia
        if settings.sepia {
            let sr = r * 0.393 + g * 0.769 + b * 0.189;
            let sg = r * 0.349 + g * 0.686 + b * 0.168;
            let sb = r * 0.272 + g * 0.534 + b * 0.131;
            r = sr; g = sg; b = sb;
        }

        rgba[idx] = r.clamp(0.0, 255.0) as u8;
        rgba[idx + 1] = g.clamp(0.0, 255.0) as u8;
        rgba[idx + 2] = b.clamp(0.0, 255.0) as u8;
    }
}
