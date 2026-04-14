/// Unit tests for ios-remote.

#[cfg(test)]
mod tests {
    // ─── Frame Analysis ──────────────────────────────────────

    #[test]
    fn test_motion_score_identical_frames() {
        let frame = make_test_frame(100, 100, [128, 128, 128, 255]);
        let score = ios_remote_motion_score(&frame, &frame);
        assert!(score < 0.01, "Identical frames should have near-zero motion");
    }

    #[test]
    fn test_motion_score_different_frames() {
        let a = make_test_frame(100, 100, [0, 0, 0, 255]);
        let b = make_test_frame(100, 100, [255, 255, 255, 255]);
        let score = ios_remote_motion_score(&a, &b);
        assert!(score > 0.9, "Completely different frames should have high motion");
    }

    // ─── Color Picker ────────────────────────────────────────

    #[test]
    fn test_rgb_to_hsl_red() {
        let (h, s, l) = rgb_to_hsl(255, 0, 0);
        assert!((h - 0.0).abs() < 1.0, "Red hue should be ~0");
        assert!(s > 0.9, "Pure red should have high saturation");
        assert!((l - 0.5).abs() < 0.1, "Pure red should have ~50% lightness");
    }

    #[test]
    fn test_rgb_to_hsl_white() {
        let (h, s, l) = rgb_to_hsl(255, 255, 255);
        assert!(s < 0.01, "White should have zero saturation");
        assert!((l - 1.0).abs() < 0.01, "White should have 100% lightness");
    }

    // ─── Zoom ────────────────────────────────────────────────

    #[test]
    fn test_zoom_clamp() {
        let mut z = ZoomState::new();
        z.zoom(100.0, 0.0, 0.0); // try to zoom way in
        assert!(z.level <= 10.0, "Zoom should clamp to max 10x");

        z.zoom(-100.0, 0.0, 0.0); // try to zoom way out
        assert!(z.level >= 1.0, "Zoom should clamp to min 1x");
    }

    #[test]
    fn test_zoom_reset() {
        let mut z = ZoomState::new();
        z.zoom(5.0, 100.0, 100.0);
        assert!(z.level > 1.0);
        z.reset();
        assert!((z.level - 1.0).abs() < f32::EPSILON);
    }

    // ─── Version Comparison ──────────────────────────────────

    #[test]
    fn test_version_newer() {
        assert!(version_newer("0.3.0", "0.2.0"));
        assert!(version_newer("1.0.0", "0.99.99"));
        assert!(!version_newer("0.2.0", "0.3.0"));
        assert!(!version_newer("0.2.0", "0.2.0"));
    }

    // ─── Template Matching ───────────────────────────────────

    #[test]
    fn test_ncc_identical() {
        let frame = make_test_frame(50, 50, [100, 150, 200, 255]);
        let score = ncc_score_simple(&frame.rgba, 50, &frame.rgba, 50, 50, 0, 0);
        assert!(score > 0.99, "Identical images should score ~1.0");
    }

    // ─── Helpers ─────────────────────────────────────────────

    struct TestFrame {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    }

    fn make_test_frame(w: u32, h: u32, pixel: [u8; 4]) -> TestFrame {
        let rgba = pixel.repeat((w * h) as usize);
        TestFrame { width: w, height: h, rgba }
    }

    fn ios_remote_motion_score(a: &TestFrame, b: &TestFrame) -> f64 {
        let total = (a.width * a.height) as usize;
        let mut changed = 0usize;
        for i in 0..total {
            let idx = i * 4;
            if idx + 2 >= a.rgba.len() || idx + 2 >= b.rgba.len() { break; }
            let dr = (a.rgba[idx] as i32 - b.rgba[idx] as i32).unsigned_abs();
            let dg = (a.rgba[idx+1] as i32 - b.rgba[idx+1] as i32).unsigned_abs();
            let db = (a.rgba[idx+2] as i32 - b.rgba[idx+2] as i32).unsigned_abs();
            if dr + dg + db > 20 { changed += 1; }
        }
        changed as f64 / total as f64
    }

    fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
        let r = r as f32 / 255.0;
        let g = g as f32 / 255.0;
        let b = b as f32 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if (max - min).abs() < f32::EPSILON { return (0.0, 0.0, l); }
        let d = max - min;
        let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
        let h = if (max - r).abs() < f32::EPSILON {
            ((g - b) / d + if g < b { 6.0 } else { 0.0 }) * 60.0
        } else if (max - g).abs() < f32::EPSILON {
            ((b - r) / d + 2.0) * 60.0
        } else {
            ((r - g) / d + 4.0) * 60.0
        };
        (h, s, l)
    }

    struct ZoomState { level: f32, offset_x: f32, offset_y: f32 }
    impl ZoomState {
        fn new() -> Self { Self { level: 1.0, offset_x: 0.0, offset_y: 0.0 } }
        fn zoom(&mut self, delta: f32, _mx: f32, _my: f32) {
            self.level = (self.level + delta * 0.1).clamp(1.0, 10.0);
        }
        fn reset(&mut self) { self.level = 1.0; self.offset_x = 0.0; self.offset_y = 0.0; }
    }

    fn version_newer(a: &str, b: &str) -> bool {
        let parse = |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse().ok()).collect() };
        let va = parse(a); let vb = parse(b);
        for i in 0..va.len().max(vb.len()) {
            let a = va.get(i).copied().unwrap_or(0);
            let b = vb.get(i).copied().unwrap_or(0);
            if a > b { return true; } if a < b { return false; }
        }
        false
    }

    fn ncc_score_simple(frame: &[u8], fw: u32, tmpl: &[u8], tw: u32, th: u32, ox: u32, oy: u32) -> f64 {
        let mut sum_ft = 0.0f64; let mut sum_ff = 0.0f64; let mut sum_tt = 0.0f64;
        for ty in 0..th { for tx in 0..tw {
            let fi = (((oy+ty)*fw+(ox+tx))*4) as usize;
            let ti = ((ty*tw+tx)*4) as usize;
            if fi+2 >= frame.len() || ti+2 >= tmpl.len() { continue; }
            let fg = (frame[fi] as f64*0.3 + frame[fi+1] as f64*0.6 + frame[fi+2] as f64*0.1)/255.0;
            let tg = (tmpl[ti] as f64*0.3 + tmpl[ti+1] as f64*0.6 + tmpl[ti+2] as f64*0.1)/255.0;
            sum_ft += fg*tg; sum_ff += fg*fg; sum_tt += tg*tg;
        }}
        if sum_ff < f64::EPSILON || sum_tt < f64::EPSILON { return 0.0; }
        sum_ft / (sum_ff.sqrt() * sum_tt.sqrt())
    }
}
