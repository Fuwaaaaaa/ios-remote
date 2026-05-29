use super::wda_client::{WdaClient, default_wda_client};
use super::{Frame, FrameBus, template_match};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

// Automation macro system: record and replay sequences of actions.
//
// A macro is a series of timed actions (tap, swipe, wait, screenshot)
// that can be saved to JSON and replayed. Useful for repetitive tasks
// like game farming, form filling, or testing flows.

/// NCC match score above which `WaitForScreen` considers the template present.
const WAIT_FOR_SCREEN_THRESHOLD: f64 = 0.85;
/// How often `WaitForScreen` re-samples the latest frame (>33ms frame tick).
const WAIT_POLL_MS: u64 = 100;
/// Upper bound on a single `Repeat`'s iteration count (guards hand-edited JSON).
const MAX_REPEAT: u32 = 10_000;
/// Maximum `Repeat` nesting depth before bailing.
const MAX_REPEAT_DEPTH: u32 = 8;
/// Absolute ceiling on total actions executed by one macro run.
const MAX_TOTAL_ACTIONS: u64 = 1_000_000;

/// Abstraction over the touch-input backend so the macro engine can be unit
/// tested with a spy. `WdaClient` is the production implementation. `Send +
/// Sync` so a macro can run on a spawned task holding `&dyn TouchInput`.
pub trait TouchInput: Send + Sync {
    fn tap(&self, x: u32, y: u32) -> Result<(), String>;
    fn swipe(&self, x1: u32, y1: u32, x2: u32, y2: u32, duration_ms: u64) -> Result<(), String>;
    fn long_press(&self, x: u32, y: u32, duration_ms: u64) -> Result<(), String>;
}

impl TouchInput for WdaClient {
    fn tap(&self, x: u32, y: u32) -> Result<(), String> {
        WdaClient::tap(self, x, y).map_err(|e| e.to_string())
    }
    fn swipe(&self, x1: u32, y1: u32, x2: u32, y2: u32, duration_ms: u64) -> Result<(), String> {
        WdaClient::swipe(self, x1, y1, x2, y2, duration_ms).map_err(|e| e.to_string())
    }
    fn long_press(&self, x: u32, y: u32, duration_ms: u64) -> Result<(), String> {
        WdaClient::long_press(self, x, y, duration_ms).map_err(|e| e.to_string())
    }
}

/// Source of the latest decoded frame, used by `WaitForScreen`. `FrameBus` is
/// the production implementation; `EmptyFrames` is the no-op for input-only
/// callers (which never use `WaitForScreen`).
pub trait FrameSource: Send + Sync {
    fn latest(&self) -> Option<Arc<Frame>>;
}

impl FrameSource for FrameBus {
    fn latest(&self) -> Option<Arc<Frame>> {
        self.latest_frame()
    }
}

/// Frame source that never yields a frame (for input-only macro execution).
pub struct EmptyFrames;

impl FrameSource for EmptyFrames {
    fn latest(&self) -> Option<Arc<Frame>> {
        None
    }
}

/// Sleep helper that skips the timer entirely for a zero delay — avoids piling
/// up thousands of no-op timer registrations inside a large `Repeat`.
async fn sleep_ms(ms: u64) {
    if ms > 0 {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

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

    /// Execute the macro against the default WebDriverAgent endpoint (picks up
    /// `IOS_REMOTE_WDA_URL` or the 127.0.0.1:8100 default). Input-only — uses
    /// no frame source, so `WaitForScreen` would time out; call
    /// [`Macro::execute_full`] with a real `FrameSource` (e.g. the `FrameBus`)
    /// when `WaitForScreen` must observe the screen.
    pub async fn execute(&self) -> Result<(), String> {
        self.execute_with(&default_wda_client()).await
    }

    /// Execute with a specific WDA client but no frame source (input-only).
    pub async fn execute_with(&self, client: &WdaClient) -> Result<(), String> {
        self.execute_full(client, &EmptyFrames).await
    }

    /// Execute the macro with both a touch-input backend and a frame source.
    ///
    /// Actions that do not require device input (Wait, Screenshot) run even if
    /// input is unreachable; input actions bubble the error up. `WaitForScreen`
    /// polls `frames` until the template matches or the timeout elapses;
    /// `Repeat` re-runs the preceding actions, guarded against runaway counts
    /// and deep nesting.
    pub async fn execute_full(
        &self,
        input: &dyn TouchInput,
        frames: &dyn FrameSource,
    ) -> Result<(), String> {
        info!(name = %self.name, actions = self.actions.len(), "Executing macro");

        let mut executed: u64 = 0;
        for (i, action) in self.actions.iter().enumerate() {
            self.run_one(action, i, input, frames, 0, &mut executed)
                .await?;
        }

        info!(name = %self.name, "Macro completed");
        Ok(())
    }

    /// Execute a single action. Recursive (via `Box::pin`) for `Repeat`, bounded
    /// by `MAX_REPEAT_DEPTH` and `MAX_TOTAL_ACTIONS`.
    async fn run_one(
        &self,
        action: &MacroAction,
        idx: usize,
        input: &dyn TouchInput,
        frames: &dyn FrameSource,
        depth: u32,
        executed: &mut u64,
    ) -> Result<(), String> {
        *executed += 1;
        if *executed > MAX_TOTAL_ACTIONS {
            return Err(format!(
                "macro exceeded action budget ({MAX_TOTAL_ACTIONS})"
            ));
        }

        match action {
            MacroAction::Tap { x, y, delay_ms } => {
                sleep_ms(*delay_ms).await;
                info!(step = idx, x, y, "Macro: tap");
                input.tap(*x, *y)?;
            }
            MacroAction::Swipe {
                x1,
                y1,
                x2,
                y2,
                duration_ms,
                delay_ms,
            } => {
                sleep_ms(*delay_ms).await;
                info!(step = idx, "Macro: swipe ({},{})→({},{})", x1, y1, x2, y2);
                input.swipe(*x1, *y1, *x2, *y2, *duration_ms)?;
            }
            MacroAction::LongPress {
                x,
                y,
                duration_ms,
                delay_ms,
            } => {
                sleep_ms(*delay_ms).await;
                info!(step = idx, x, y, duration_ms, "Macro: long press");
                input.long_press(*x, *y, *duration_ms)?;
            }
            MacroAction::Wait { duration_ms } => {
                info!(step = idx, duration_ms, "Macro: wait");
                sleep_ms(*duration_ms).await;
            }
            MacroAction::Screenshot { delay_ms } => {
                sleep_ms(*delay_ms).await;
                info!(
                    step = idx,
                    "Macro: screenshot (delegated to screenshot feature)"
                );
                // Actual frame grab is owned by screenshot::save_frame via
                // the API layer; here we just mark the intent so replays
                // can time screenshots relative to input.
            }
            MacroAction::WaitForScreen {
                template_path,
                timeout_ms,
                region,
            } => {
                self.wait_for_screen(idx, template_path, *timeout_ms, *region, frames)
                    .await?;
            }
            MacroAction::Repeat {
                count,
                actions_back,
            } => {
                self.run_repeat(idx, *count, *actions_back, input, frames, depth, executed)
                    .await?;
            }
        }
        Ok(())
    }

    /// Poll the latest frame until `template_path` appears (NCC ≥ threshold) or
    /// `timeout_ms` elapses. Reuses the shared `template_match` matcher.
    async fn wait_for_screen(
        &self,
        idx: usize,
        template_path: &str,
        timeout_ms: u64,
        region: Option<(u32, u32, u32, u32)>,
        frames: &dyn FrameSource,
    ) -> Result<(), String> {
        let (tpl, tw, th) = template_match::load_template(template_path)
            .map_err(|e| format!("WaitForScreen: {e}"))?;
        info!(step = idx, template = %template_path, timeout_ms, "Macro: WaitForScreen");

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if let Some(frame) = frames.latest() {
                // Guard against a template larger than the search area — the
                // matcher's `search_w - template_w` would otherwise underflow.
                let (rw, rh) = match region {
                    Some((_, _, w, h)) => (w, h),
                    None => (frame.width, frame.height),
                };
                if tw > rw || th > rh {
                    return Err(format!(
                        "WaitForScreen: template {tw}x{th} larger than search area {rw}x{rh}"
                    ));
                }
                let r = template_match::find_template(
                    &frame,
                    &tpl,
                    tw,
                    th,
                    region,
                    WAIT_FOR_SCREEN_THRESHOLD,
                );
                if r.matched {
                    info!(step = idx, score = r.score, "Macro: WaitForScreen matched");
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(format!("WaitForScreen timed out after {timeout_ms}ms"));
            }
            tokio::time::sleep(Duration::from_millis(WAIT_POLL_MS)).await;
        }
    }

    /// Re-run the `actions_back` actions preceding index `idx`, `count` times.
    #[allow(clippy::too_many_arguments)]
    async fn run_repeat(
        &self,
        idx: usize,
        count: u32,
        actions_back: u32,
        input: &dyn TouchInput,
        frames: &dyn FrameSource,
        depth: u32,
        executed: &mut u64,
    ) -> Result<(), String> {
        if depth >= MAX_REPEAT_DEPTH {
            return Err(format!("Repeat nesting too deep (max {MAX_REPEAT_DEPTH})"));
        }
        let back = (actions_back as usize).min(idx);
        if back == 0 {
            warn!(
                step = idx,
                "Macro: Repeat with no preceding actions — skipping"
            );
            return Ok(());
        }
        let count = if count > MAX_REPEAT {
            warn!(
                step = idx,
                count,
                max = MAX_REPEAT,
                "Macro: Repeat count clamped"
            );
            MAX_REPEAT
        } else {
            count
        };
        let start = idx - back;
        info!(step = idx, count, back, "Macro: Repeat");
        for _ in 0..count {
            for j in start..idx {
                // Box the recursive future so `run_one`'s type stays sized.
                Box::pin(self.run_one(&self.actions[j], j, input, frames, depth + 1, executed))
                    .await?;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counting touch-input spy — methods take `&self`, so it uses atomics.
    #[derive(Default)]
    struct Spy {
        taps: AtomicUsize,
        swipes: AtomicUsize,
        holds: AtomicUsize,
    }
    impl TouchInput for Spy {
        fn tap(&self, _x: u32, _y: u32) -> Result<(), String> {
            self.taps.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn swipe(&self, _: u32, _: u32, _: u32, _: u32, _: u64) -> Result<(), String> {
            self.swipes.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn long_press(&self, _: u32, _: u32, _: u64) -> Result<(), String> {
            self.holds.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct OneFrame(Arc<Frame>);
    impl FrameSource for OneFrame {
        fn latest(&self) -> Option<Arc<Frame>> {
            Some(self.0.clone())
        }
    }

    fn tap(delay_ms: u64) -> MacroAction {
        MacroAction::Tap {
            x: 1,
            y: 1,
            delay_ms,
        }
    }

    fn mac(actions: Vec<MacroAction>) -> Macro {
        Macro {
            name: "t".into(),
            description: String::new(),
            actions,
        }
    }

    fn uniform_frame(w: u32, h: u32, color: [u8; 4]) -> Arc<Frame> {
        Arc::new(Frame {
            width: w,
            height: h,
            rgba: color
                .iter()
                .copied()
                .cycle()
                .take((w * h * 4) as usize)
                .collect(),
            timestamp_us: 0,
            h264_nalu: None,
        })
    }

    fn temp_png(tag: &str, w: u32, h: u32, color: [u8; 4]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "ios_remote_test_{}_{}.png",
            tag,
            std::process::id()
        ));
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba(color));
        img.save(&path).expect("write temp template");
        path
    }

    #[tokio::test]
    async fn repeat_reruns_preceding_action() {
        let spy = Spy::default();
        let m = mac(vec![
            tap(0),
            MacroAction::Repeat {
                count: 3,
                actions_back: 1,
            },
        ]);
        m.execute_full(&spy, &EmptyFrames).await.expect("ok");
        // 1 original + 3 repeats = 4 taps.
        assert_eq!(spy.taps.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn repeat_clamps_actions_back_to_history() {
        let spy = Spy::default();
        let m = mac(vec![
            tap(0),
            tap(0),
            MacroAction::Repeat {
                count: 1,
                actions_back: 99,
            },
        ]);
        m.execute_full(&spy, &EmptyFrames).await.expect("ok");
        // back clamps to 2 (only two preceding) → 2 original + 2 repeated = 4.
        assert_eq!(spy.taps.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn repeat_zero_actions_back_is_noop() {
        let spy = Spy::default();
        let m = mac(vec![
            tap(0),
            MacroAction::Repeat {
                count: 5,
                actions_back: 0,
            },
        ]);
        m.execute_full(&spy, &EmptyFrames).await.expect("ok");
        assert_eq!(spy.taps.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn repeat_count_is_capped() {
        let spy = Spy::default();
        let m = mac(vec![
            tap(0),
            MacroAction::Repeat {
                count: u32::MAX,
                actions_back: 1,
            },
        ]);
        m.execute_full(&spy, &EmptyFrames).await.expect("ok");
        assert_eq!(spy.taps.load(Ordering::SeqCst), 1 + MAX_REPEAT as usize);
    }

    #[tokio::test]
    async fn nested_repeat_exceeding_depth_errors() {
        let spy = Spy::default();
        let m = mac(vec![
            tap(0),
            MacroAction::Repeat {
                count: 1,
                actions_back: 1,
            },
        ]);
        let mut executed = 0u64;
        // Invoke the Repeat directly at the max depth → must error, not recurse.
        let res = m
            .run_one(
                &m.actions[1],
                1,
                &spy,
                &EmptyFrames,
                MAX_REPEAT_DEPTH,
                &mut executed,
            )
            .await;
        assert!(res.is_err(), "expected depth error, got {res:?}");
    }

    #[tokio::test]
    async fn wait_for_screen_matches_present_template() {
        let color = [60, 120, 220, 255];
        let frames = OneFrame(uniform_frame(120, 120, color));
        let tpl = temp_png("match", 40, 40, color);
        let m = mac(vec![MacroAction::WaitForScreen {
            template_path: tpl.to_string_lossy().into_owned(),
            timeout_ms: 2000,
            region: None,
        }]);
        let res = m.execute_full(&Spy::default(), &frames).await;
        let _ = std::fs::remove_file(&tpl);
        assert!(res.is_ok(), "uniform template should match, got {res:?}");
    }

    #[tokio::test]
    async fn wait_for_screen_times_out_when_absent() {
        // All-black frame → NCC is always 0 → never matches.
        let frames = OneFrame(uniform_frame(120, 120, [0, 0, 0, 255]));
        let tpl = temp_png("absent", 40, 40, [60, 120, 220, 255]);
        let m = mac(vec![MacroAction::WaitForScreen {
            template_path: tpl.to_string_lossy().into_owned(),
            timeout_ms: 60,
            region: None,
        }]);
        let res = m.execute_full(&Spy::default(), &frames).await;
        let _ = std::fs::remove_file(&tpl);
        assert!(
            res.is_err() && res.as_ref().unwrap_err().contains("timed out"),
            "expected timeout, got {res:?}"
        );
    }

    #[tokio::test]
    async fn wait_for_screen_template_larger_than_region_errors() {
        let frames = OneFrame(uniform_frame(120, 120, [10, 20, 30, 255]));
        let tpl = temp_png("toobig", 60, 60, [10, 20, 30, 255]);
        let m = mac(vec![MacroAction::WaitForScreen {
            template_path: tpl.to_string_lossy().into_owned(),
            timeout_ms: 1000,
            region: Some((0, 0, 40, 40)),
        }]);
        let res = m.execute_full(&Spy::default(), &frames).await;
        let _ = std::fs::remove_file(&tpl);
        assert!(
            res.is_err() && res.as_ref().unwrap_err().contains("larger than"),
            "expected size guard, got {res:?}"
        );
    }

    #[tokio::test]
    async fn wait_for_screen_missing_template_errors() {
        let frames = OneFrame(uniform_frame(50, 50, [1, 2, 3, 255]));
        let m = mac(vec![MacroAction::WaitForScreen {
            template_path: "does/not/exist_zzz.png".into(),
            timeout_ms: 1000,
            region: None,
        }]);
        let res = m.execute_full(&Spy::default(), &frames).await;
        assert!(res.is_err(), "missing template should error");
    }
}
