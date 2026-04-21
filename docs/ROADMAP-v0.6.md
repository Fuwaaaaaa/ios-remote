# Roadmap — v0.6.0

Deferred / new work captured after the v0.5.0 release freeze (2026-04-21).
Tracked here instead of the plan file so it survives plan turnover. Items are
loosely grouped by shape of work.

## Deferred from v0.5.0

### VR overlay (originally plan C6)
- Add the `openvr` crate dep behind `--features vr`
- Call `openvr::init()` and create a Dashboard-style overlay
- Convert FrameBus RGBA → OpenGL/D3D texture
- Run the SDK loop on a blocking thread (SteamVR is sync)
- README: hardware requirement (SteamVR-compatible headset) and driver notes
- Acceptance: frames land in a floating panel in SteamVR with <80ms latency

### Session replay decode (SessionPlayer.play)
- Wire `openh264` (or `ffmpeg` subprocess) to decode each indexed NAL → RGBA
- Add `SessionPlayer::play()` that publishes decoded frames to a FrameBus at
  original timing, with `seek(ts)` honoring bookmarks
- Web Dashboard: new `/replay` page with play / pause / scrub / bookmark list
- REST: `POST /api/replay/load {path}`, `/play`, `/pause`, `/seek {ts_us}`
- Acceptance: loading a recording made by SessionRecorder plays back in the
  display window at ~the original frame rate

### Stream Deck button loop
- Run the HID event loop we can now reach via `try_open_device()`
- Map button press → `CommandPalette::execute(action_id)` through a shared
  command registry (currently actions are free-form strings)
- Render button labels + PNG icons to the LCD
- Hot-reload on layout file change
- Acceptance: pressing the `Screenshot` button saves a PNG; pressing `Record`
  toggles the RecordingController

### Local Whisper end-to-end
- Wire `Transcriber` into the subtitle overlay (draw_subtitles already
  exists); today nothing calls `transcribe_chunk`
- Capture audio — we have no source yet since AirPlay audio was removed; need
  a Windows WASAPI loopback or the user's microphone
- Acceptance: with a ggml model at the documented path, live speech produces
  subtitle lines in the display window

## Backlog discovered during v0.5.0 smoke/review

### 68-feature stocktake
Many files under `src/features/` are scaffolded but never reached from any
startup path. Examples from the scan: `app_detector`, `mouse_gesture`,
`multi_device`, `drag_drop`, `presentation`, `pdf_export`, `tts`,
`video_filter`, `vr_overlay`. For each:
- Decide: promote (wire up), quarantine (mark `#[cfg(feature = "experimental")]`),
  or delete
- Goal: shrink the "dead-but-compiled" surface by at least 50% — keeps
  clippy surface + compile time small and avoids misleading README claims
- No-merge until a pass/axe decision is made for every feature file

### Cargo.lock in source
Currently gitignored. For a binary crate this means CI builds aren't byte-
reproducible. Track the lock file and confirm `cargo build --locked` stays
green on CI.

### Cross-process activity indicator
When recording or session replay is running, expose a hotkey / tray indicator.
Right now, a user can start recording via API and have no visible feedback in
the display window.

### `iproxy` auto-spawn for macros
WDA needs USB port 8100 forwarded before macros work. Today the user runs
`iproxy` manually. We could:
- Detect WDA service via usbmuxd `com.facebook.WebDriverAgentRunner.xctrunner`
- Spin up an internal tunnel through our existing usbmuxd `connect()` path
- Drop the curl-to-http subprocess in favor of a direct TCP client over the
  tunnel — cleaner than `iproxy`

### CI matrix expansion
- Add a job that sets up LLVM and builds with `--features whisper` to prevent
  `whisper-rs-sys` bitrot
- Add `cargo audit --deny warnings` as a soft-fail job so new upstream advisories
  surface on every PR

### Release hygiene
- Enable rustfmt as a hard gate (currently `continue-on-error: true`)
- Add a `cargo deny` pass (licenses, banned crates, duplicate deps)
- Sign Windows .exe with an Authenticode cert once we have one

## Explicitly not in v0.6 scope

- Cross-platform support (macOS / Linux usbmuxd paths) — out until someone
  volunteers to own that OS-specific code
- H.265 / HEVC — v0.5.0 only targets H.264 from screenshotr
- Multi-user / cloud dashboards — local tool by design
