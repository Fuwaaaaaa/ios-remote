//! Periodic synthetic-subtitle injector. Called by `--synthetic` mode so the
//! Subtitles dashboard card and `/api/subtitles` endpoint return populated
//! data without real audio capture.

use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

const LINES: &[&str] = &[
    "WELCOME TO IOS REMOTE SYNTHETIC MODE",
    "NO IPHONE REQUIRED FOR THIS DEMO",
    "RECORDING AND REPLAY ARE FULLY WIRED",
    "TRY HOTKEY S TO SAVE A SCREENSHOT",
    "TRY HOTKEY F2 TO START RECORDING",
    "DASHBOARD AT HTTP LOCALHOST 8080",
];

pub fn spawn(push: Arc<dyn Fn(String) + Send + Sync>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        // First tick fires immediately — push something right away so the
        // dashboard isn't blank for the first 5 seconds.
        let mut idx: usize = 0;
        loop {
            tick.tick().await;
            push(LINES[idx % LINES.len()].to_string());
            idx = idx.wrapping_add(1);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[tokio::test]
    async fn pump_pushes_first_line_immediately() {
        // tokio::time::interval fires the first tick immediately, so a short
        // wall-clock wait is enough to observe one push. We don't pause time
        // here because the project doesn't enable tokio's test-util feature.
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let cap = captured.clone();
        let push: Arc<dyn Fn(String) + Send + Sync> =
            Arc::new(move |s: String| cap.lock().expect("lock").push(s));
        let handle = spawn(push);
        tokio::time::sleep(Duration::from_millis(150)).await;
        handle.abort();
        let v = captured.lock().expect("lock");
        assert!(!v.is_empty(), "expected ≥1 push within 150ms, got {}", v.len());
        assert!(
            LINES.contains(&v[0].as_str()),
            "first push should be one of LINES, got {}",
            v[0]
        );
    }
}
