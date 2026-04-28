# Changelog

All notable changes to this project are documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the
project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- **`--diag` one-shot diagnostic mode.** Walks the
  usbmuxd → lockdownd → screenshotr chain on every connected device and
  prints raw GetValue results plus the StartService XML response to
  stdout, then exits. Designed to be run as
  `ios-remote.exe --diag > diag.txt 2>&1` and pasted into an issue when
  reporting "Trust" / "screen does not display" failures.
- **iOS 17+ structural-incompatibility warning.** The capture loop and
  the stall warning now flag iOS 17+ devices explicitly. This build
  speaks the classic usbmuxd/lockdownd/screenshotr stack only — it does
  *not* implement ReadPairRecord, StartSession, the post-session TLS
  upgrade, or the Personalized Developer Disk Image mount via RemoteXPC
  that iOS 17+ requires for screenshotr. The warning makes the failure
  mode explicit instead of looping silently with a misleading
  "USB session ended — will retry".
- **iOS 17+ Phase 2 scaffolding (`--features ios17`).** Optional Cargo
  feature pulls in the `idevice` crate (jkcoxson, pure Rust, `=0.1.58`
  pinned) with `usbmuxd + tcp + aws-lc + screenshotr` features, plus a
  new `usb::idevice_bridge` adapter exposing
  `connect_by_udid → device_info / start_service` over Pair record +
  StartSession + TLS. Default builds are unaffected. The bridge is not
  yet wired into the runtime path; routing in `mod.rs` and the iOS 17+
  capture loop land in a follow-up after `--diag` results from real
  hardware confirm which services need re-routing.

### Changed
- **Lockdownd StartService failures dump the full response.**
  Previously only the `Error` field was surfaced; now `Error`,
  `ErrorString`, `Domain`, and `Type` are logged structurally, and the
  raw XML plist body is emitted at `debug` level. This turns the
  long-standing "trust"-looking error into something actually
  diagnosable.
- **Stall warning now includes the most recent DeviceInfo** (UDID,
  model, iOS version) and switches to the iOS 17+ guidance message when
  the cached device is unsupported.

### Notes
- This release does *not* fix the underlying iOS 17+ incompatibility —
  that requires implementing pairing/session/TLS plus a RemoteXPC
  tunnel and PDI mount, or switching to a different transport
  (QuickTime Sync over CoreMediaIO). Tracked for a future release.

## [0.7.1] — 2026-04-27

Post-release security + correctness audit on v0.7.0. No new features;
no behavior change for users beyond the log line and subtitle window.

### Security
- **API Bearer token no longer printed verbatim at startup.** The
  `info` log now shows a masked form (`head…tail`); the full token is
  only emitted at `debug` level. Captured stdout / log files no longer
  leak the token, which is critical when the dashboard is exposed via
  `--lan`.
- **`OPENAI_API_KEY` no longer passed on the curl command line.** The
  OpenAI transcription fallback now feeds curl a `-K -` config via
  stdin, so `Authorization: Bearer …` never appears in the process
  command line (visible to other local users on Windows via
  `tasklist /v` / WMI / Process Explorer). Values are escaped per
  `curl(1)`.
- **Per-pid temp WAV is removed after each transcription call.**
  Captured audio used to linger in `%TEMP%`; it is now deleted on both
  success and failure paths. Set `IOS_REMOTE_KEEP_AUDIO_TMP=1` to
  retain it for debugging.

### Fixed
- **Resampler stall guard.** `current_rate.max(1)` clamp in the
  transcription pump so a (theoretical) zero sample rate from a
  misbehaving cpal backend can't stall the loop.
- **Subtitle visibility window honors `chunk_secs`.** Previously fixed
  at 3 s, which blanked out before the next 5–10 s chunk arrived. Now
  uses `chunk_secs * 1000`, floored at 3 s.

### Tests
- +5 unit tests: `mask_token` head/tail behavior, `escape_curl_config_value`
  escaping rules, subtitle duration floor + honor. Full suite:
  62 passed, 0 failed. `cargo clippy --all-targets -- -D warnings`
  clean for both the default and `audio_capture` feature builds.

## [0.7.0] — 2026-04-27

The v0.7 theme is **the audio pipeline**. We landed the missing capture
source so the existing transcription / subtitle / overlay code finally
has something to consume — and fixed two correctness bugs surfaced by
post-merge review (lock contention during inference, model reload per
chunk) before the feature went anywhere near a tag.

### Added
- **WASAPI loopback audio capture + Whisper subtitles end-to-end** —
  `src/features/audio_capture.rs` introduces an `AudioBus` /
  `AudioCapture::spawn` pair that opens the default Windows output device
  in WASAPI loopback mode via `cpal`, with mic fallback when no output
  device is available. A transcription pump down-mixes to mono, resamples
  to 16 kHz, and feeds 5-second windows into `transcribe_blocking` on
  `tokio::task::spawn_blocking`. Local whisper.cpp consumes the f32 buffer
  directly; the OpenAI HTTP path keeps working for non-`whisper` builds.
  The display loop renders the live subtitle bar using a richer 5x7
  bitmap font (full A–Z, a–z, common punctuation) shared with the stats
  overlay. New routes `GET /api/audio/status` and `GET /api/subtitles`
  expose state to the dashboard. Configurable via the new `[audio]` block
  in `ios-remote.toml` (`source`, `chunk_secs`, `language`).
- **`audio_capture` feature flag** — gates the cpal dep so default builds
  stay slim; `whisper` now implies `audio_capture` so the end-to-end
  pipeline is reachable.
- **CI: `whisper` build job** — Windows runner with LLVM 17 installed
  builds `--features whisper`, with `LIBCLANG_PATH` wired explicitly for
  whisper-rs-sys / bindgen. Closes the v0.6 roadmap blocker that gated
  whisper work on bitrot risk.
- **Whisper context cache** — process-global `OnceLock<Option<Arc<_>>>`
  loads the ~140 MB ggml model exactly once. A `None` entry records
  prior init failure so we don't retry on every chunk.

### Changed
- `Transcriber` exposes `now_ms()` so the capture pump and display loop
  share a single monotonic clock for subtitle timestamps and visibility
  windows.
- WAV byte layout extracted to `audio_viz::pcm16_to_wav_bytes` /
  `f32_to_wav_bytes` and reused by the OpenAI transcription fallback.
- `run_display` now accepts `Option<Arc<Mutex<Transcriber>>>`. `None` =
  no-op (default builds).
- Unknown `audio.source` strings now warn at startup instead of silently
  disabling capture — typo'd values land in the log instead of looking
  like a feature regression.

### Fixed
- **Pump no longer holds `Mutex<Transcriber>` across whisper inference.**
  Pre-fix, every chunk locked the transcriber for the entire ~200ms–
  several-second duration of `state.full(...)` (or curl HTTP). Display
  thread + `/api/*` handlers also lock that mutex, so the UI froze and
  the API stalled with each chunk. Inference now runs on
  `spawn_blocking` with the lock released; the pump only re-locks for
  microseconds to read `now_ms()` and call `add_subtitle`.
- **OpenAI fallback temp file is now per-pid** (`ios_remote_audio_<pid>.wav`)
  so concurrent ios-remote instances don't trample each other's
  uploads.

## [Unreleased]

_(no entries yet)_

## [0.6.0] — 2026-04-27

The big v0.6 theme is **dispatch unification** — REST, Stream Deck, and
hotkeys all flow through a single `command_palette::execute` surface.
**22 of 33 Command Palette actions** dispatch live; the rest return a
structured `409 not_dispatchable` with a clear "phase" reason so the
gaps are honest and easy to plan against.

### Added
- **Live H.264 encoder** — `H264Encoder` subscribes to the shared `FrameBus`,
  feeds each RGBA frame into an `ffmpeg` subprocess
  (`-c:v libx264 -preset ultrafast -tune zerolatency`), and republishes the
  resulting Annex-B NAL units back on the bus. This wires the screenshotr
  PNG→RGBA capture path through to every H.264 consumer (recording, RTMP,
  `SessionRecorder`) that was previously a no-op. The encoder auto-respawns
  on resolution change and falls back silently (single warning) if ffmpeg
  is missing. Loopback-safe: encoder output carries empty `rgba`, and the
  display / encoder both ignore rgba-empty frames.
- **Session replay playback** — `SessionPlaybackController` spawns ffmpeg
  (`-f h264 -i pipe:0 -f rawvideo -pix_fmt rgba pipe:1`), feeds recorded
  NAL units at the session's original frame rate, and publishes decoded
  RGBA frames on the shared `FrameBus` so the existing display window
  renders them without code changes. New REST endpoints:
  `GET /api/replay/sessions`, `POST /api/replay/{load,play,pause,seek}`.
  The Web Dashboard gains a Replay card with a session picker, play /
  pause / seek controls, and bookmark shortcuts. ffmpeg stays an optional
  runtime dependency (seam added via `with_ffmpeg_bin` for testability).
  Seeking while playing is rejected — callers pause, seek, then resume.
- **Command Palette dispatch** — `command_palette::execute(action_id, &ApiState)`
  is the single dispatch surface for the 33 Command Palette actions. Wired
  this release: screenshot, screenshot_clipboard, record_start, record_stop,
  ocr, ocr_clipboard, ai_describe, qr_scan, check_update, startup_toggle,
  quit, web_dashboard, settings, firewall_setup, translate, zoom_in,
  zoom_out, zoom_reset, game_mode, stats_toggle, annotation_clear,
  color_pick. Returns `CommandError` with a phase tag for the rest.
- **REST `POST /api/command/{id}` + `GET /api/commands`** — Bearer-protected;
  status mapping is 200 success / 404 unknown / 409 not dispatchable / 503 no
  frame / 500 handler failed. Used by the Web Dashboard and any external
  client that wants the same surface as Stream Deck.
- **Stream Deck HID event loop** (`--features stream_deck`) — opens the
  first attached device, edge-detects rising-edge presses, dispatches each
  through `command_palette::execute` on a fresh OS thread (no HID-loop
  backpressure from slow handlers like `ai_describe`).
- **Activity indicator** — display window title shows `● REC` / `▶ REPLAY` /
  `[PiP]` when the corresponding lifecycle is active. Title is only re-set
  when the composed string changes — no per-frame Win32 churn.
- **Shared display state** — new `features::display_state::DisplayState`
  collects zoom + game mode + annotations + stats visibility +
  PendingInteractive under one `Arc<std::sync::Mutex<…>>` shared by the
  display thread, REST, and Stream Deck. Source frame dimensions are
  pushed into ZoomState every frame so dispatch-driven zoom works without
  knowing the device geometry.
- **`color_pick` interactive flow** — dispatch arms `pending = ColorPick`;
  the display loop's mouse handler completes the action on the next
  left-click rising edge, translating buffer-space coords back to source
  coords (zoom-offset compensated), and stores the picked color (hex /
  rgb / hsl) in `DisplayState.last_picked`.
- **`iproxy` auto-spawn** — startup probes `127.0.0.1:8100`; if nothing's
  listening, spawns `iproxy 8100 8100 -u <UDID>` so WDA macros work
  without manual tunneling. Failure modes (port bound, iproxy not on
  PATH, spawn error) all log + continue — never fatal.
- **CI: whisper build job + cargo-deny pass** — a Windows + LLVM 17 job
  runs `cargo build --features whisper` on every PR (closes whisper-rs-sys
  bitrot risk). A separate soft-fail `cargo deny` job covers licenses,
  banned crates, and source drift; new `deny.toml` declares the
  permissive license set we ship under.

### Changed
- **`command_palette::search`** — replaced per-call `Box::leak` with
  `OnceLock<Vec<Command>>` (was leaking ~3 KB per invocation; trivial in
  practice but uncapped over a long session).
- **`run_display`** — now takes `RecordingController`, `SessionPlaybackController`,
  and `Arc<Mutex<DisplayState>>` so the title bar's activity indicator and
  the zoom transform / pending-action handler can read state every frame.
- **README** — adds a Command Palette dispatch section documenting the
  REST status codes and the 22/33 action count, plus Stream Deck and
  Activity Indicator subsections.

### Deferred to a follow-up
- **WASAPI loopback for Whisper** — the capture side (transcribe_chunk)
  was wired in v0.5, but live system-audio capture stayed out of v0.6 to
  keep the COM init / resampling path on a hardware-verified review.
  Tracked in `docs/ROADMAP-v0.6.md`.
- **Phase C remainder** — annotation_rect/arrow/text, ruler, privacy_add,
  privacy_clear are multi-click state machines; this release lays the
  PendingInteractive groundwork (color_pick) and stops there.
- **`pip_toggle` runtime flip** — minifb sets `topmost` at window
  creation; runtime flipping needs a Win32 `SetWindowPos` hack against
  the display window HWND, scoped to a separate PR.
- **Picker-required commands** — `macro_run`, `lua_run`, `network_diag`,
  `gif_save` need caller-supplied arguments; they wait for an
  `execute(action, args, state)` signature bump.

## [0.5.0] — 2026-04-21

Hardening, real-feature wiring, and a Windows-only declaration. Binary
compatibility: no migrations needed; existing `ios-remote.toml` files load
with defaults filling in the new network fields.

### Removed
- **AirPlay receiver** — the entire `src/airplay/` subtree (11 files,
  ~1,548 LOC) was dead since the v0.4.0 USB switch. Deleted along with its
  unused crypto dependencies.

### Added
- **USB reconnect** — exponential backoff (1s → 16s max) between attempts,
  with a periodic "still waiting" warning so idle state is visible.
- **Multi-device selection** — `--list-devices` enumerates attached iPhones,
  `--device <UDID>` pins the target; fallback warns when picking the first of
  several.
- **API Bearer authentication** — a 32-byte URL-safe token is generated on
  first launch, persisted to `ios-remote.toml`, and required on every
  `/api/*` request (constant-time compare, optional `?token=` query fallback).
- **Recording lifecycle** — `POST /api/recording/start` and `/stop` are wired
  to a new `RecordingController`; the output path is returned in the JSON
  response.
- **SessionPlayer** — parses `session.json` + `bookmarks.json` + Annex-B
  `video.h264`, exposes per-NAL iteration and proportional seek. RGBA
  playback (needs a decoder) is deferred.
- **Stream Deck (HID)** behind `--features stream_deck`. The earlier draft
  pointed at a non-existent local WebSocket; the new code uses the
  `elgato-streamdeck` crate.
- **Whisper.cpp local transcription** behind `--features whisper`, resolving
  the model path from `IOS_REMOTE_WHISPER_MODEL`.
- **WebDriverAgent macro input** — `MacroAction::{Tap, Swipe, LongPress}` now
  dispatches to WDA over usbmuxd-tunnelled HTTP (curl, no new Rust deps).
- **Scheduler unit tests** — Once / Daily / Interval / midnight-wrap.
- **Config unit tests** — save/load round-trip, token generation, alphabet.
- **CI** — GitHub Actions workflow builds + tests on `windows-latest` and
  enforces `cargo clippy --all-targets -- -D warnings`.
- **CHANGELOG.md** (this file).

### Changed
- **Default bind address** — `0.0.0.0` → `127.0.0.1`. LAN exposure is now an
  explicit opt-in via `--lan` or `network.lan_access = true`.
- **Web Dashboard** — token is injected inline so same-origin fetch calls
  automatically attach `Authorization: Bearer`.
- **Scheduler precision** — 2-second window → 1-second window, with midnight
  wrap handled. `Daily` tracks `last_fired_date` so tasks no longer re-fire
  on subsequent ticks within the window or on the same day.
- **MJPEG share** — now binds localhost only and logs on port conflict
  instead of panicking.
- **Documentation** — `README.md` gains a troubleshooting table, macro /
  WDA setup, network config block, optional-feature build hints; `TESTING.md`
  expanded to 10 end-to-end scenarios.

### Fixed
- **Panic surface** — 12 `unwrap()/expect()` call sites that could crash the
  process on port conflicts, lock poisoning, Lua init failure, or malformed
  h264 are replaced with typed `Result` handling.
- **`TESTING.md` path typo** — `test_branch/ios_remort` → `test/ios-remote`.

### Security
- Local-by-default binding (see above).
- Bearer token required on all `/api/*` routes regardless of bind address.
- Constant-time token comparison to mitigate timing attacks.

### Windows-only
- `build.rs` fails the build on non-Windows targets with a clear diagnostic,
  because `AppleMobileDeviceService` (the Windows-only usbmuxd provider from
  iTunes / Apple Devices) is a hard runtime dependency.

## [0.4.0]
- Switch from AirPlay to USB Type-C using the usbmuxd protocol (screenshotr).

## [0.3.0]
- 80 files, all feature scaffolding complete.

## [0.2.0] and earlier
- Initial AirPlay receiver, RTSP server, mDNS announcement, pairing.
