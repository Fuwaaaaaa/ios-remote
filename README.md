# ios-remote

AirPlay screen mirroring receiver + iPhone integration tool for Windows, written entirely in Rust.

## Features

### Core — AirPlay Mirroring
- **Screen mirroring** via AirPlay protocol (mDNS + RTSP + H.264)
- **NTP time sync** for frame synchronization
- **FairPlay pairing** (transient mode, no PIN)
- **Audio receiving** (RTP/AAC-ELD)

### Display
- **Picture-in-Picture** mode (always-on-top, resizable)
- **Aspect ratio** preserving letterbox
- **Stats overlay** (FPS, latency, resolution, bitrate)
- **Touch overlay** (tap ripple, swipe trail, long-press ring)
- **Hotkeys**: `S` = screenshot, `Q`/`Esc` = quit

### Recording & Streaming
- **Video recording** to H.264 file (`--record`)
- **Screenshot** to PNG (hotkey or API)
- **OBS virtual camera** via named pipe (`--obs`)
- **RTMP live streaming** via ffmpeg (`--rtmp <url>`)

### Screen Analysis
- **OCR** text extraction (tesseract, Japanese + English)
- **AI vision** screen understanding (Claude API)
- **Notification capture** — auto-detect iOS notification banners
- **Motion detection** — frame diff analysis

### Automation
- **Macros** — JSON-based record/replay (tap, swipe, wait, screenshot)
- **Multi-device** — manage multiple iPhones simultaneously

### iPhone USB Integration (via idevice crate)
- Device info (battery, storage, iOS version)
- File transfer (AFC protocol)
- Syslog relay
- Crash log retrieval

### Web Dashboard
- **REST API** at `http://localhost:8080/api/`
- **Browser dashboard** at `http://localhost:8080/`
- Real-time stats, screenshot, OCR, AI describe, macro control

## Quick Start

```bash
# Basic start
cargo run

# With recording + PiP mode
cargo run -- --record --pip

# With RTMP streaming
cargo run -- --rtmp "rtmp://live.twitch.tv/app/YOUR_KEY"

# With config file
cargo run -- --config

# Custom name and ports
cargo run -- --name "My PC" --port 7000 --web-port 8080
```

Then on your iPhone:
1. Connect to the same Wi-Fi network
2. Open Control Center (swipe down from top-right)
3. Tap "Screen Mirroring"
4. Select "ios-remote"

## REST API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/status` | GET | Connection status |
| `/api/stats` | GET | Stream statistics |
| `/api/screenshot` | POST | Take screenshot |
| `/api/recording/start` | POST | Start recording |
| `/api/recording/stop` | POST | Stop recording |
| `/api/ocr` | POST | Extract text from screen |
| `/api/ai/describe` | POST | AI screen description |
| `/api/config` | GET/POST | Read/update configuration |
| `/api/history` | GET | Connection history |
| `/api/macros` | GET | List saved macros |
| `/api/macros/run` | POST | Execute a macro |

## Configuration

Create `ios-remote.toml` or use `--config` flag:

```toml
[receiver]
name = "ios-remote"
port = 7000
max_width = 1920
max_height = 1080
max_fps = 60

[display]
pip_mode = false
show_stats = true
show_touch_overlay = true

[recording]
auto_record = false
output_dir = "recordings"

[features]
obs_virtual_camera = false
notification_capture = true
ocr = false
ai_vision = false
```

## Architecture

```
iPhone ──Wi-Fi──> [mDNS Discovery]
                       │
                  [RTSP Server :7000]
                  ├── /info
                  ├── /pair-setup
                  ├── /pair-verify
                  ├── /fp-setup
                  ├── SETUP
                  └── RECORD
                       │
              ┌────────┴────────┐
              │                 │
    [H.264 Stream :7100]  [Audio RTP :7100]
              │
        [OpenH264 Decode]
              │
         [FrameBus] ──broadcast──> Display Window
              │                    Recording
              │                    Screenshot
              │                    OBS Virtual Camera
              │                    RTMP Streaming
              │                    Notification Capture
              │                    OCR / AI Vision
              │
    [Web Dashboard :8080] ←── REST API
```

## Requirements

- Rust 1.80+
- Windows 10/11 (primary target)
- Same Wi-Fi network as iPhone
- Optional: tesseract-ocr (for OCR)
- Optional: ffmpeg (for RTMP streaming)
- Optional: ANTHROPIC_API_KEY (for AI vision)

## License

MIT
