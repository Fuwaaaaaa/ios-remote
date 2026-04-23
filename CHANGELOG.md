# Changelog

All notable changes to this project are documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the
project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
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
