# Roadmap — v0.6.0

Deferred / new work captured after the v0.5.0 release freeze (2026-04-21).
Tracked here instead of the plan file so it survives plan turnover. Items are
loosely grouped by shape of work.

## Deferred from v0.5.0

### VR overlay (originally plan C6)
`src/features/vr_overlay.rs` already has a `VrOverlay` skeleton that drains
broadcast frames to avoid backpressure (lines 51-59) but is otherwise a
no-op with `unimplemented!()` behind its feature gate. Remaining work:

- Add the `openvr` crate dep behind `--features vr` (not in Cargo.toml yet)
- Call `openvr::init()` and create a Dashboard-style overlay
- Convert FrameBus RGBA → OpenGL/D3D texture
- Run the SDK loop on a blocking thread (SteamVR is sync)
- README: hardware requirement (SteamVR-compatible headset) and driver notes
- Acceptance: frames land in a floating panel in SteamVR with <80ms latency

### Session replay decode
Today `SessionPlayer` (src/features/session_replay.rs) only exposes
`nalu(index)` and `seek_proportional()` — there is no `play()` method, and
neither `openh264` nor `ffmpeg` is in Cargo.toml. Work breaks into three
layers, none of which are started:

1. **Decoder selection (design decision, do first)** — compare `openh264`
   crate (pure Rust bindings, Cisco license surface) vs an `ffmpeg`
   subprocess (no build-time C deps, harder to pipe RGBA back). Write the
   choice + rationale into this file before coding.
2. **`SessionPlayer::play()`** — publish decoded RGBA to a FrameBus at
   original timing, with `seek(ts)` honoring bookmarks.
3. **REST + UI** — `POST /api/replay/load {path}`, `/play`, `/pause`,
   `/seek {ts_us}`; Web Dashboard `/replay` page with play / pause / scrub /
   bookmark list.

Acceptance: loading a SessionRecorder output plays back in the display
window at ~the original frame rate.

### Stream Deck button loop
Precondition: `CommandPalette` (src/devtools/command_palette.rs) currently
has only `search()` / `all_commands()` returning static `Command` structs —
**there is no `execute(action_id)` dispatch**. Do that first, otherwise the
Stream Deck loop has nothing to call.

1. **CommandPalette dispatch** — add `action_id → handler` registry +
   `execute(action_id)` entry point. Migrate existing free-form action
   strings in `StreamDeckIntegration::on_press` (stream_deck.rs:51-55) to
   typed action ids.
2. **HID loop** — use the already-defined `try_open_device()`
   (stream_deck.rs:74-89) and call `CommandPalette::execute` on press.
3. **LCD rendering** — button labels + PNG icons.
4. **Hot-reload** on layout file change.

Acceptance: pressing the `Screenshot` button saves a PNG; pressing `Record`
toggles the RecordingController.

### Local Whisper end-to-end
The decode side is already wired: `transcribe_chunk` →
`add_subtitle` → `draw_subtitles` in src/features/audio_transcription.rs
(lines 26-146). The `#[allow(dead_code)]` exists because **there is no
audio source**: AirPlay audio was removed in v0.5, and no WASAPI/mic
capture code lives under src/. The remaining work is almost entirely
capture, not transcription:

1. **Audio capture (the actual work)** — Windows WASAPI loopback for
   system audio, with mic as a fallback. Feeds PCM chunks into
   `transcribe_chunk`.
2. **CI whisper build** — add an LLVM-enabled job that builds with
   `--features whisper`. Blocker for landing any whisper work without
   silent bitrot (see "CI matrix expansion" below).

Acceptance: with a ggml model at the documented path and WASAPI loopback
running, live system speech produces subtitle lines in the display window.

## Backlog discovered during v0.5.0 smoke/review

### 68-feature stocktake
Eight files under `src/features/` are declared in `features/mod.rs` but
never reached from any startup path — **541 lines dead-but-compiled**:
`app_detector`, `mouse_gesture`, `multi_device`, `drag_drop`,
`presentation`, `pdf_export`, `tts`, `video_filter` (`vr_overlay` is
tracked separately above).

Apply the policy below as a single PR, not one file at a time:

- **Promote** (wire up now) — files whose capability is advertised in
  README/CHANGELOG or shipped in a UI panel
- **Quarantine** behind `#[cfg(feature = "experimental")]` (off by default
  in v0.6) — "nice-to-have" scaffolds that are worth keeping on ice
- **Delete** — everything else; the git history is enough record

Goal: shrink the dead-but-compiled surface by at least 50% to keep clippy
surface + compile time small and avoid misleading README claims.

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

**Done in v0.6 prep:**
- ~~Flip rustfmt to a hard gate~~ — `continue-on-error` removed; codebase
  normalized in a single fmt pass (78 files)
- ~~Add `cargo audit --deny warnings`~~ — landed as a soft-fail job on
  ubuntu-latest

**Still open (blocks whisper work):**
- Add a job that sets up LLVM and builds with `--features whisper` to
  prevent `whisper-rs-sys` bitrot — currently deferred by comment in
  `.github/workflows/test.yml`

### Release hygiene
- Add a `cargo deny` pass (licenses, banned crates, duplicate deps)
- Sign Windows .exe with an Authenticode cert once we have one

## Explicitly not in v0.6 scope

- Cross-platform support (macOS / Linux usbmuxd paths) — out until someone
  volunteers to own that OS-specific code
- H.265 / HEVC — v0.5.0 only targets H.264 from screenshotr
- Multi-user / cloud dashboards — local tool by design
