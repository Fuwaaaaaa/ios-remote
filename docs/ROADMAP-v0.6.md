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

### Session replay decode — DONE

Closed in v0.6 prep. `SessionPlaybackController` in
`src/features/session_replay.rs` now spawns ffmpeg and republishes decoded
RGBA frames on the shared `FrameBus`; the existing display window picks
them up through the same path as live capture. REST endpoints under
`/api/replay/*` and a Replay section in the Web Dashboard expose the
controls end-to-end. ffmpeg remains an optional runtime dep (documented in
the README Optional Dependencies table).

Follow-up closed: **H.264 source wiring**. `src/features/h264_encoder.rs`
runs an always-on RGBA→H.264 encoder that republishes NAL units onto the
bus, so recording / replay / RTMP consumers see populated
`Frame.h264_nalu`. Same ffmpeg-optional posture: missing binary → single
warn + silent no-op, no panic.

#### Decision record (kept for posterity)

**Decoder decision: ffmpeg subprocess** (decided 2026-04-21).

| Axis | ffmpeg subprocess | `openh264` crate |
|---|---|---|
| Runtime dependency | Already documented in README (RTMP line 281); end-users already install it. **No new dep.** | Would add a new build+runtime dep. Cisco binary royalty shipped via their hosted blob. |
| Build surface | Zero new Rust deps; zero new C deps in the cargo build. | New crate + either vendored C lib (build cost) or dynamic load (runtime cost). |
| Existing pattern | `src/features/streaming.rs:35-86` already spawns ffmpeg with `-f h264 -i pipe:0` and pipes NALUs in. **Paste-and-adapt**. | No precedent in this codebase; would introduce a decoder abstraction. |
| Output plumbing | Read raw RGBA frames from stdout (`-f rawvideo -pix_fmt rgba pipe:1`). Slight framing complexity (fixed `width*height*4` chunks). | Returns decoded frames directly via Rust API — cleaner on paper. |
| License | ffmpeg install is the user's problem; we call it as a tool, no linking. | BSD-2 crate + Cisco blob; need a NOTICE line for redistributions. |
| Performance | Fine for playback (~60fps RGBA pipe is bandwidth-bound, not CPU-bound). | Fine; possibly faster for short clips (no process startup). |

The deciding factor is that ffmpeg is already a paid cost. Adding
`openh264` would double the h.264 decode surface area for no gain on the
Windows-only, playback-only workload. The stdout rawvideo read is the
only real downside and it's a ~30-line chunk reader.

**Implementation order:**

1. **`SessionPlayer::play()`** — mirror `streaming.rs:rtmp_stream` shape:
   spawn ffmpeg with `-f h264 -i pipe:0 -f rawvideo -pix_fmt rgba pipe:1`,
   feed `nalu(i)` frames into stdin on a writer task, read fixed-size
   RGBA chunks from stdout on a reader task, publish to `FrameBus` with
   original timing. `seek(ts)` honors bookmarks.
2. **REST + UI** — `POST /api/replay/load {path}`, `/play`, `/pause`,
   `/seek {ts_us}`; Web Dashboard `/replay` page with play / pause /
   scrub / bookmark list.

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

### 68-feature stocktake — DONE

Closed in v0.6 prep. Nine dead-but-compiled files (657 LOC) resolved:

- **Deleted** (147 LOC): `multi_device` (contradicted README "1 台ずつ"
  single-device posture), `drag_drop` (TODO stubs only), `tts`
  (PowerShell SAPI one-liner, no UI)
- **Quarantined** behind `experimental` feature (510 LOC): `app_detector`,
  `benchmark`, `mouse_gesture`, `pdf_export`, `presentation`,
  `video_filter`. `benchmark` + `video_filter` travel as a pair since the
  former drives the latter.
- **README cleanup**: removed "アプリ使用時間" and "ベンチマーク" bullets
  that advertised quarantined-now features.
- **CI**: added `cargo build --features experimental` so quarantined code
  does not bitrot silently.

Default build is 657 LOC lighter; the design work survives under an
explicit flag. `vr_overlay` remains tracked separately in "VR overlay"
above (covered by its own roadmap item).

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
