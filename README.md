# ios-remote

<p align="center">
  <strong>English</strong> · <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <strong>Mirror &amp; control your iPhone's screen on your PC over USB Type-C</strong><br>
  Pure Rust / <strong>Windows 10 / 11 only</strong> / MIT License
</p>

> ⚠️ **Platform:** This tool is **Windows 10 / 11 only**. The build is rejected on macOS / Linux by `build.rs` because it depends on `AppleMobileDeviceService`, which ships with iTunes / Apple Devices on Windows.

---

## Overview

Plug your iPhone into your PC over USB and the screen mirrors to a desktop window in real time. No Wi-Fi. No jailbreak.

```
iPhone ──USB-C──> PC (ios-remote) ──> Display window
                                  ──> Recording / Screenshots
                                  ──> Web Dashboard
```

## Quick Start

```bash
# Build
cargo build

# Run (plug the iPhone in over USB first)
cargo run

# PiP mode (always-on-top mini window)
cargo run -- --pip

# Recording on
cargo run -- --record

# List attached devices (to find a UDID)
cargo run -- --list-devices

# Pin to a specific iPhone by UDID
cargo run -- --device 00008120-001A2B3C4D5E6F78

# Expose the dashboard on the LAN (Bearer token required)
cargo run -- --lan
```

### First launch

You'll see a log block like the one below right after startup. Note the `API token` value — it's also persisted to the config file.

```
INFO  API token (Bearer): kQ3m7dF2-sLaP9xR0cT8vB1n
INFO  Local-only mode — use --lan to expose on all interfaces.
INFO  Web dashboard: http://127.0.0.1:8080
```

- Defaults bind to `127.0.0.1`, so the dashboard is reachable only from the same PC.
- `--lan` flips the bind to `0.0.0.0` so other LAN hosts can connect. The API token is mandatory.
- Override the token via the `IOS_REMOTE_API_TOKEN` env var, or pin it in `ios-remote.toml` under `[network] api_token`.

### Requirements

| Item | Detail |
|------|--------|
| **Windows 10 / 11** | The only supported OS |
| **USB Type-C cable** | Lightning-to-C is fine too |
| **iTunes / Apple Devices** | Installed on Windows (provides the AppleMobileDeviceService / usbmuxd driver) |
| **"Trust" prompt** | Tap "Trust" on the iPhone the first time you connect |
| **Rust 1.80+** | For building from source |

### Platform support

- ✅ **Windows 10 / 11** — Native
- ❌ **macOS / Linux** — Not supported (`build.rs` rejects the build)
- ❌ **AirPlay mode** — Removed in v0.4.0; USB Type-C only

## Features

### Core — USB screen mirroring
- **Direct USB Type-C** — No Wi-Fi, low latency
- **usbmuxd protocol** — Apple's official USB mux protocol
- **lockdownd session** — Device info + service activation
- **screenshotr capture** — Real-time screen frames from the iPhone

### Display
- **PiP mode** — Always-on-top mini window (`--pip`)
- **Aspect-ratio preserved** — Letterboxed; no stretch
- **Stats overlay** — FPS / latency / resolution in real time
- **Touch overlay** — Ripple animation at the tap point
- **Hotkeys** — `S`=screenshot, `Q`/`Esc`=quit

### Recording & Capture
- **Video recording** — H.264 stream saved to a file (`--record`)
- **Screenshots** — PNG (hotkey or API)
- **GIF** — Save the last N seconds as an animated GIF
- **Time-lapse** — Periodic frame capture
- **Motion-triggered recording** — Record only when the screen changes (saves disk)

### Screen Analysis
- **OCR** — Tesseract-based text extraction (Japanese + English)
- **AI screen understanding** — "What's on screen?" via the Claude API
- **Notification capture** — Auto-detect and store iOS banner notifications
- **QR / barcode scanner** — Picks up codes anywhere on screen
- **Color picker** — HEX / RGB / HSL at the mouse cursor

### Automation
- **Macros** — Record/replay tap / swipe / wait actions in JSON
- **Lua scripting** — Lua 5.4 for advanced automation (`--features lua`)
- **Gesture library** — Pre-built pinch / rotate / 3-finger swipe presets
- **Voice commands** — Say "screenshot" to capture
- **Scheduled tasks** — Cron-style periodic execution

### Visual Tools
- **Annotations** — Arrows / rectangles / text / freehand on top of the frame
- **Ruler** — Pixel-distance measurement
- **Device frame** — Overlay an iPhone bezel for screenshot / video polish
- **iOS safe-area markers** — Notch / Dynamic Island guides
- **Design grid** — 8pt / 4pt grid overlay
- **Color-blindness simulation** — Render the screen as different vision types see it
- **Privacy mode** — Blur or pixelate specific regions
- **Watermark** — Stamped on recordings / live streams

### Streaming & Sharing
- **RTMP** — Twitch / YouTube live via ffmpeg
- **OBS virtual camera** — Named-pipe video into OBS
- **MJPEG** — Stream the screen to any browser
- **Imgur instant share** — One-key screenshot → upload → URL
- **Notification forwarding** — Push detected notifications to Discord / Slack / Telegram

### Analytics
- **Touch heatmap** — Visualize click frequency
- **Frame-diff highlight** — Color-code per-frame deltas
- **Session replay** — Recorded sessions play back from the Web Dashboard's Replay card (`/api/replay/*`). Decoding uses ffmpeg (see Optional Dependencies below)

### Developer Tools
- **Command palette** — Fuzzy search across 33 commands
- **Protocol analyzer** — Detailed RTSP / usbmuxd message logs
- **Network diagnostics** — Ping / latency / jitter
- **Bandwidth throttling** — Cap network usage
- **Connection timeline** — Chronological event view

### System Integration
- **Web Dashboard** — Browser-based status + control (http://localhost:8080)
- **REST API** — 18 endpoints for programmatic control
- **Config file** — Persistent TOML config
- **Connection history** — Logs previously connected devices
- **Run on Windows startup** — Registry-based auto-start
- **System tray** — Minimize to tray
- **Auto-updater** — Notifies on new GitHub Releases
- **Portable mode** — Run from a USB stick
- **i18n** — Japanese / English / Chinese / Korean
- **Themes** — Dark / Light / Midnight / Nature

## Troubleshooting

| Symptom | Things to check |
|---------|------------------|
| `Cannot connect to usbmuxd` | iTunes / Apple Devices installed and `AppleMobileDeviceService` running as a Windows service |
| `No iPhone connected` | USB cable, port, the "Trust" tap on the iPhone |
| Screen freezes | Whether the USB-C cable supports data (charge-only cables won't work) |
| Endless reconnect right after launch | Run `--list-devices` and pin the UDID with `--device <UDID>` |
| `401 Unauthorized` in the dashboard | Use the API token from the startup log; open the dashboard from `/` (it embeds the token) instead of typing API URLs |
| `Failed to bind Web dashboard` | Pick a different port with `-w <PORT>` |
| Multiple iPhones at once | One device at a time today; switch with `--device` |

## Macro setup (sending iOS input)

`MacroAction::Tap` / `Swipe` / `LongPress` send taps to the iPhone via [WebDriverAgent (WDA)](https://github.com/appium/WebDriverAgent). The `screenshotr` service is read-only, so input requires a sideloaded WDA signed with an Apple Developer cert.

1. Build WDA in Xcode and install it on the iPhone
2. Launch WDA on the iPhone once and confirm it's listening on port 8100
3. On the PC, forward the USB port (e.g. `iproxy 8100 8100`)
4. Set `IOS_REMOTE_WDA_URL=http://127.0.0.1:8100` (the default value too)
5. Trigger macros via `POST /api/macros/run` or the `F7` hotkey

If WDA isn't running, `Tap` / `Swipe` / `LongPress` actions return errors but the process won't crash — `Wait` and `Screenshot` actions keep working.

## Session Replay

Sessions saved under `recordings/` (a directory containing `session.json` / `bookmarks.json` / `video.h264`) play back from the Replay card in the Web Dashboard.

### Steps

1. Record a session with `F2` or `POST /api/recording/start` → stopping creates `recordings/session_YYYYMMDD_HHMMSS/`.
2. Open the dashboard and hit **Refresh** in the Replay card to populate the list.
3. Pick a session and click **Load** — header info (resolution / frame count / length) and bookmarks appear.
4. **Play** to start, **Pause** to stop.
5. Use the slider or bookmark buttons to seek (seek-while-playing is rejected; pause first, seek, then resume).

### ffmpeg dependency

Decoding spawns an ffmpeg subprocess (`-f h264 -i pipe:0 -f rawvideo -pix_fmt rgba pipe:1`). If ffmpeg isn't installed, `POST /api/replay/play` returns `{ "status": "error", "error": "spawn ffmpeg: ..." }`. See Optional Dependencies below.

### Known limitations

- Seek positions are mapped proportionally (NAL-unit granularity, coarse timestamp accuracy)
- Playback speed is fixed at 1.0×
- With ffmpeg missing, recording / replay / RTMP all become no-ops (the encoder also uses ffmpeg)

## Web Dashboard

`http://localhost:8080` opens a real-time dashboard. The token is inlined into the dashboard HTML and attached automatically to fetch calls. Cards: Status / Actions / Replay / Log / Connection History.

### REST API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/status` | GET | Connection state |
| `/api/stats` | GET | Stream statistics |
| `/api/screenshot` | POST | Take a screenshot |
| `/api/recording/start` | POST | Start recording |
| `/api/recording/stop` | POST | Stop recording |
| `/api/ocr` | POST | Extract text |
| `/api/ai/describe` | POST | AI screen understanding |
| `/api/config` | GET/POST | Read / write config |
| `/api/history` | GET | Connection history |
| `/api/macros` | GET | List macros |
| `/api/macros/run` | POST | Run a macro |
| `/api/replay/sessions` | GET | List recorded sessions |
| `/api/replay/load` | POST | Load a session |
| `/api/replay/play` | POST | Start playback |
| `/api/replay/pause` | POST | Pause |
| `/api/replay/seek` | POST | Seek (only while paused) |
| `/api/audio/status` | GET | Current audio source + active subtitles |
| `/api/subtitles` | GET | Subtitle history (up to 50 entries) |
| `/api/commands` | GET | All Command Palette commands |
| `/api/command/{id}` | POST | Dispatch a Command Palette action |

### Command Palette dispatch

The 33 Command Palette actions can be invoked from REST / Stream Deck / internal hotkeys through a single dispatch path. The HTTP status returned by `/api/command/{id}` reveals what the handler did:

| Status | Meaning | Body shape |
|--------|---------|------------|
| `200` | Success | `{ "ok": true, "action": "<id>", "message": "<result>" }` |
| `404` | Unknown action id | `{ "ok": false, "error": "unknown_action", "action": "<id>" }` |
| `409` | Action recognized but not yet dispatchable (waiting on interactive input, requires args, scheduled for a future PR) | `{ "ok": false, "error": "not_dispatchable", "action": "<id>", "reason": "<why>" }` |
| `503` | No frame received yet (analysis-style commands need an attached device) | `{ "ok": false, "error": "no_frame", "reason": "<why>" }` |
| `500` | Handler failed | `{ "ok": false, "error": "handler_failed", "action": "<id>", "message": "<details>" }` |

As of v0.6, **22 / 33 actions dispatch live** (screenshot / recording / OCR / AI describe / QR / zoom / game_mode / annotation_clear / web_dashboard / settings / firewall_setup / translate / startup_toggle / quit / color_pick and more). The rest are Phase C continuations (annotation_*, ruler, privacy_*) and arg-requiring commands (macro_run, lua_run, network_diag, gif_save) — they return `409` with an explicit reason.

### Stream Deck integration

Building with `--features stream_deck` starts an event loop that maps each Elgato Stream Deck button to a Command Palette action. Default 8-button layout: screenshot / record_start / record_stop / ocr / gif_save / pip_toggle / game_mode / ai_describe. Every press flows through Command Palette dispatch, so REST and Stream Deck behavior stays in lockstep.

### Activity indicator

The display window's title bar reflects recording / replay state:
- `ios-remote — USB Mirror` — idle
- `ios-remote — ● REC` — recording
- `ios-remote — ▶ REPLAY` — session replay running
- `ios-remote — ● REC · ▶ REPLAY · [PiP]` — combined

Title updates follow REST / Stream Deck-driven state changes, so you can read the current state without watching logs.

## Configuration

Customize via `ios-remote.toml`:

```toml
[receiver]
name = "ios-remote"

[display]
pip_mode = false
show_stats = true
show_touch_overlay = true

[recording]
auto_record = false
output_dir = "recordings"

[network]
bind_address = "127.0.0.1"   # Set to "0.0.0.0" to expose on the LAN. --lan does the same.
lan_access = false            # true forces bind_address to 0.0.0.0.
api_token = ""                # Empty → auto-generated and written back on first launch.

[features]
notification_capture = true
ocr = false
ai_vision = false
```

## Architecture

```
┌─────────┐     USB Type-C      ┌──────────────┐
│ iPhone  │ ──────────────────>  │  usbmuxd     │
└─────────┘                      │ (port 27015) │
                                 └──────┬───────┘
                                        │
                                 ┌──────▼───────┐
                                 │  lockdownd   │
                                 │ (port 62078) │
                                 └──────┬───────┘
                                        │
                                 ┌──────▼───────┐
                                 │ screenshotr  │
                                 │ (PNG capture)│
                                 └──────┬───────┘
                                        │
                                 ┌──────▼───────┐
                                 │   FrameBus   │──> Display Window
                                 │ (broadcast)  │──> Recording
                                 │              │──> Screenshot / GIF
                                 │              │──> OCR / AI / QR
                                 │              │──> OBS / RTMP / MJPEG
                                 │              │──> Heatmap / Analysis
                                 └──────────────┘
                                        │
                                 ┌──────▼───────┐
                                 │ Web Dashboard│
                                 │  :8080       │
                                 └──────────────┘
```

## Hotkeys

| Key | Action |
|-----|--------|
| `S` | Screenshot |
| `Q` / `Esc` | Quit |
| `P` | Toggle PiP |
| `R` | Reset zoom |
| `F2` | Start / stop recording |
| `F3` | Run OCR |
| `F4` | Toggle stats overlay |
| `F5` | Toggle game mode |
| `G` | Save GIF |
| `I` | Color picker |
| `M` | Ruler |
| `Scroll` | Zoom |

## Optional Dependencies

| Tool | Purpose | Install |
|------|---------|---------|
| tesseract-ocr | OCR text extraction | [tesseract](https://github.com/tesseract-ocr/tesseract) |
| ffmpeg | RTMP streaming / recording transcode / Session Replay decode | [ffmpeg.org](https://ffmpeg.org) |
| `ANTHROPIC_API_KEY` | AI screen understanding | [anthropic.com](https://console.anthropic.com) |
| `OPENAI_API_KEY` | Audio transcription (OpenAI Whisper API) | [openai.com](https://platform.openai.com) |
| `IMGUR_CLIENT_ID` | Imgur instant share | [imgur.com/account/settings/apps](https://imgur.com/account/settings/apps) |
| ggml-base.bin | Local Whisper (no API key required) | [whisper.cpp models](https://huggingface.co/ggerganov/whisper.cpp) |

## Audio capture & transcription

Captures the PC's system audio over **WASAPI loopback**, runs it through Whisper, and overlays live subtitles on the display window. Falls back to the default microphone when no output device is available.

### Build flags

```sh
# OpenAI Whisper API path (needs OPENAI_API_KEY, no local model required)
cargo build --features audio_capture

# Local whisper.cpp path (offline, ggml model required)
cargo build --features whisper   # implies audio_capture
```

### Configuration

The `[audio]` block in `ios-remote.toml` (existing files get default values automatically):

```toml
[audio]
source = "loopback"   # "loopback" | "mic" | "off"
chunk_secs = 5
# language = "ja"     # Whisper auto-detects when omitted
```

### Environment variables

| Variable | Purpose |
|----------|---------|
| `IOS_REMOTE_WHISPER_MODEL` | Path to the ggml model (default `%APPDATA%\ios-remote\models\ggml-base.bin`) |
| `OPENAI_API_KEY` | API fallback when local whisper isn't available |

### REST

| Endpoint | Description |
|----------|-------------|
| `GET /api/audio/status` | Current source + active subtitles |
| `GET /api/subtitles` | Subtitle history (up to 50 entries) |

## Project Structure

```
src/
├── main.rs              Entry point + CLI
├── config.rs            TOML settings + connection history
├── error.rs             Error types
├── usb/                 USB connection (core)
│   ├── usbmuxd.rs       usbmuxd protocol client
│   ├── lockdown.rs       lockdownd client
│   ├── screen_capture.rs screenshotr capture loop
│   └── device.rs         device management
├── features/            All feature modules (65 default + 6 experimental + 1 audio_capture-gated)
│   ├── display.rs        Window rendering
│   ├── recording.rs      Video recording
│   ├── screenshot.rs     PNG capture
│   ├── ocr.rs            Text extraction
│   ├── ai_vision.rs      Claude API vision
│   ├── audio_capture.rs  WASAPI loopback (gated behind `audio_capture`)
│   ├── ...               (55+ more modules; 6 gated behind `experimental`)
│   └── zoom.rs           Zoom & pan
├── ui/                  Web interface
│   ├── api.rs            REST API (axum)
│   └── web.rs            Browser dashboard
├── system/              OS integration
│   ├── tray.rs           System tray
│   ├── startup.rs        Auto-start
│   ├── updater.rs        Update checker
│   ├── portable.rs       Portable mode
│   └── installer.rs      NSIS script generator
├── devtools/            Developer tools
│   ├── command_palette.rs Command search
│   ├── protocol_analyzer.rs Protocol logger
│   ├── network_diag.rs   Network diagnostics
│   ├── timeline.rs       Event timeline
│   └── throttle.rs       Bandwidth control
└── idevice/             USB device integration (stubs)
    ├── device_info.rs     Device info
    ├── file_transfer.rs   File transfer (AFC)
    └── syslog.rs          System log relay
```

## Build

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# With Lua scripting
cargo build --features lua

# Stream Deck button integration (HID; requires hardware)
cargo build --features stream_deck

# WASAPI loopback audio capture (with OpenAI Whisper API fallback)
cargo build --features audio_capture

# Local Whisper transcription (requires ggml model)
cargo build --features whisper

# Experimental features (app_detector / benchmark / mouse_gesture / pdf_export / presentation / video_filter)
cargo build --features experimental

# Run tests
cargo test
```

## CI/CD

GitHub Actions runs **automated Windows builds and releases** (macOS / Linux are out of scope):

```bash
# Cut a release
git tag v0.7.0
git push --tags
```

## Contributing

1. Fork this repository
2. Create a feature branch
3. Confirm `cargo test` passes
4. Open a Pull Request

## License

MIT License — see [LICENSE](LICENSE) for details.

---

<p align="center">
  Made with Rust 🦀
</p>
