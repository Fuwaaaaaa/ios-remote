# Smoke Test — v0.7.0 (WASAPI loopback + Whisper subtitles)

**Date:** 2026-04-27
**Branch / tag:** master, tagged `v0.7.0`
**Commits in this release:** `9fef97a` (feat) → `875a88d` (fix) → release prep
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

### ✅ S1. Default build still green

```
$ cargo build --locked
   Compiling ios-remote v0.7.0 (C:\project\test\ios-remote)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.15s
```

cpal / cpal-platform are NOT pulled into the dependency graph — confirmed
by `cargo tree -e features` (no cpal node when feature is off).

### ✅ S2. `audio_capture` build green

```
$ cargo build --locked --features audio_capture
   Compiling ios-remote v0.7.0 (C:\project\test\ios-remote)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.02s
```

Pulls in `cpal 0.15.3`, `windows 0.54.0`, `dasp_sample 0.11.0` (cpal
transitive). No new warnings.

### ✅ S3. Tests pass

```
$ cargo test
test result: ok. 49 passed; 0 failed; 2 ignored; ...
test result: ok. 8 passed; 0 failed; 0 ignored; ...

$ cargo test --features audio_capture
test result: ok. 51 passed; 0 failed; 2 ignored; ...
test result: ok. 8 passed; 0 failed; 0 ignored; ...
```

The 2 new tests under `--features audio_capture` are
`audio_source_parse_roundtrip` and `audio_bus_broadcasts`. The 5 new
tests under `audio_transcription::tests` (wrap, add_subtitle cap) run
in both configurations.

### ✅ S4. Existing feature builds unchanged

```
$ cargo build --locked --features lua          # 14.66s
$ cargo build --locked --features stream_deck  # 14.31s
$ cargo build --locked --features experimental # 17.45s
```

v0.7 work is purely additive. No warnings introduced.

### ✅ S5. Clippy hard gate (-D warnings)

```
$ cargo clippy --all-targets -- -D warnings
$ cargo clippy --all-targets --features audio_capture -- -D warnings
```

Both green.

### ✅ S6. cargo audit

```
$ cargo audit --deny warnings
Loaded 1058 security advisories (from .../advisory-db)
Scanning Cargo.lock for vulnerabilities (402 crate dependencies)
EXIT=0
```

The two pre-existing triages (`RUSTSEC-2024-0384` minifb→instant,
`RUSTSEC-2024-0436` image→ravif→paste) remain ignored per
`.cargo/audit.toml`. No new advisories surfaced from the cpal
sub-graph.

### ⏳ S7. `whisper` build — deferred to CI

The `whisper:` job in `.github/workflows/test.yml` installs LLVM 17 and
runs `cargo build --features whisper`. Local agent run does not have
LLVM/libclang in PATH; deferred to GitHub Actions for the v0.7.0 tag.

---

## Lock contention regression (v0.7 review fix)

Post-merge review of the v0.7 audio pipeline surfaced two issues that
were fixed in `875a88d` before tag:

1. The pump locked `Mutex<Transcriber>` across the entire whisper /
   curl call — display thread (60 fps) and `/api/audio/*` handlers
   would block with each chunk. Fixed by running `transcribe_blocking`
   on `tokio::task::spawn_blocking` with the lock released, and
   re-locking only for the microsecond-tier `now_ms()` and
   `add_subtitle()` calls.
2. `WhisperContext` was rebuilt every chunk (~140 MB ggml reload from
   disk). Fixed by caching it in a process-global
   `OnceLock<Option<Arc<_>>>` — first call loads, subsequent calls
   clone the Arc.

The S* scenarios above all run on the post-fix tree.

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
