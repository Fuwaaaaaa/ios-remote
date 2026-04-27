# Smoke Test — v0.7 prep (WASAPI loopback + Whisper subtitles)

**Date:** 2026-04-27
**Branch:** master (post-v0.6.0)
**Tester:** agent-driven build smoke (no iPhone, no audio playback)

This is the agent-runnable record for the v0.7 headline (WASAPI loopback
audio capture → Whisper subtitles end-to-end). Hardware-dependent
sections (H*) are deferred to a human tester with speakers / a
microphone and an iPhone for the full mirror experience.

## Environment

| Item | Value |
|------|-------|
| OS | Windows 11 (harness) |
| Rust | stable (via dtolnay/rust-toolchain) |
| ggml-base.bin present | **NO** in this run |
| OPENAI_API_KEY set | **NO** in this run |
| iPhone attached | **NO** |

---

## Agent-run scenarios (no hardware required)

### S1. Default build still green

```
cargo check
```

Expected: clean — cpal + cpal-platform deps must NOT be pulled in.

### S2. `audio_capture` build green

```
cargo check --features audio_capture
```

Expected: cpal compiles; ios-remote links; no warnings beyond existing
ones.

### S3. Tests for the new bits

```
cargo test --features audio_capture audio_
```

Covers: `wrap_two_lines`, `add_subtitle_drops_oldest_past_cap`,
`audio_source_parse_roundtrip`, `audio_bus_broadcasts`.

### S4. Existing feature-set builds unchanged

```
cargo check --features lua
cargo check --features stream_deck
cargo check --features experimental
```

Expected: all green. v0.7 work is additive.

### S5. `whisper` build (CI job, not run locally)

The `whisper:` job in `.github/workflows/test.yml` installs LLVM 17 and
runs `cargo build --features whisper`. Locally this requires `LIBCLANG_PATH`
to be set; deferred to CI in this run.

---

## H — Human / hardware scenarios (deferred)

### H1. Live loopback → subtitle bar

1. `cargo run --features whisper`
2. Confirm log: `Audio capture started source=loopback sample_rate=48000 channels=2`
3. Play a YouTube clip with clear English speech for ≥10s.
4. Within ~5 s of the first sentence, the dark bar at the bottom of the
   display window shows transcribed text in white.
5. `curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8080/api/subtitles`
   returns the same lines.

### H2. Source switch via config

Edit `ios-remote.toml`:

```toml
[audio]
source = "off"
```

Restart. Bar disappears. `GET /api/audio/status` reports
`{"enabled": false, "reason": "audio_capture feature disabled or no device"}`.

### H3. Mic fallback

Disable the speaker device in the Windows Sound panel, then restart with
`source = "loopback"`. Expected: a single warn `Loopback open failed;
falling back to mic`, capture continues from the default input device.

### H4. Long session (memory bound)

Run with capture active for ≥30 minutes. `Transcriber::subtitles` is
capped at 50 entries; verify VSZ stays flat in Task Manager.

---

## Notes

- Loopback latency end-to-end (audio → subtitle visible): bounded by
  `chunk_secs * 1000` ms (default 5000 ms) plus whisper inference time
  (~200–800 ms on base.en).
- Non-48 kHz mixers (rare) trigger a one-time warn about non-integer
  resampling. Quality on 44.1 kHz is acceptable for English; non-integer
  resample uses nearest-sample selection.
- Subtitle font: 5x7 bitmap, full A–Z + a–z + common punctuation.
  Non-ASCII characters render as a hollow box (Japanese / accented Latin
  out of scope for v0.7; tracked separately).
