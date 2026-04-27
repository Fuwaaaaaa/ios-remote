# Smoke test — v0.6.0

Manual checklist for verifying a v0.6 build before tagging. Run on a
Windows host with an iPhone attached via USB Type-C and Apple Devices /
iTunes installed.

## Prerequisites

- [ ] `cargo build` succeeds with default features
- [ ] `cargo build --features stream_deck` succeeds (HID stack present)
- [ ] `cargo build --features experimental` succeeds (quarantined scaffolds compile)
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo test` passes — current target is **52 in-binary tests + 8 integration tests**

## Connection + display

1. [ ] Run `cargo run` with iPhone attached.
2. [ ] Log shows `USB receiver: device connected` within 3 s.
3. [ ] Display window opens with title `ios-remote — USB Mirror`.
4. [ ] Live screen mirrors at ~30 fps; CPU is reasonable.

## Activity indicator

5. [ ] Press `S` → screenshot saved (hotkey path) — log line confirms.
6. [ ] `curl -X POST -H "Authorization: Bearer <T>" http://127.0.0.1:8080/api/recording/start` →
       title becomes `ios-remote — ● REC`. Confirm `recordings/<ts>/video.h264` file grows.
7. [ ] `…/api/recording/stop` → title returns to `ios-remote — USB Mirror`.

## Command Palette dispatch

8. [ ] `…/api/commands` → 200 + 33 commands.
9. [ ] `…/api/command/screenshot` → 200, file saved.
10. [ ] `…/api/command/zoom_in` → 200; display visibly zooms.
       Repeat → zoom level grows. `…/api/command/zoom_reset` → 1.0x.
11. [ ] `…/api/command/game_mode` → 200, "game mode on / off" toggles.
12. [ ] `…/api/command/color_pick` → 200, message "click in the display window…".
       Click in display → log `Color picked hex=#XXXXXX rgb=...`.
13. [ ] `…/api/command/web_dashboard` → 200, default browser opens at the dashboard URL.
14. [ ] `…/api/command/bogus` → 404, body `{"ok":false,"error":"unknown_action",…}`.
15. [ ] `…/api/command/ruler` → 409 with `"reason":"Phase C in progress…"` (this is correct — wait list).
16. [ ] `…/api/command/macro_run` → 409 with `"reason":"needs caller-supplied arguments…"`.

## Stream Deck (only with `--features stream_deck` and hardware)

17. [ ] `cargo run --features stream_deck` shows `Stream Deck connected — HID loop running`.
18. [ ] Pressing button 0 saves a screenshot; button 1/2 starts/stops recording;
        button 3 runs OCR; button 7 runs AI describe. `gif_save / pip_toggle / game_mode`
        produce `stream deck press failed action=…` log lines (expected — Phase B/D follow-ups).

## iproxy auto-spawn (only with libimobiledevice on PATH)

19. [ ] Log shows either `iproxy: WDA port 8100 already forwarded — skipping spawn`
        or `iproxy: spawned tunnel for WDA on port 8100`.
20. [ ] Without iproxy on PATH: log shows `iproxy not on PATH — install libimobiledevice…`,
        no panic.

## Session replay (requires ffmpeg)

21. [ ] After a recording, `…/api/replay/sessions` lists it.
22. [ ] Web Dashboard Replay card → load → play → display shows decoded frames;
        title becomes `ios-remote — ▶ REPLAY`.

## Shutdown

23. [ ] Disconnect iPhone → log shows reconnect-attempt loop, no panic.
24. [ ] Press `Q` or `Esc` in display window → process exits cleanly.

## Known follow-ups (not blockers for v0.6 tag)

- **WASAPI loopback** (Whisper end-to-end live audio) — `whisper` feature
  still has no audio source, deferred to a follow-up PR.
- **Phase C remainder** — `annotation_rect/arrow/text`, `ruler`,
  `privacy_add` need multi-click state machines.
- **`pip_toggle` runtime** — minifb topmost runtime flipping needs a
  Win32 `SetWindowPos` hack against the display window HWND.
- **Picker-required commands** (`macro_run`, `lua_run`, `network_diag`,
  `gif_save`) — wait for an `execute(action, args, state)` signature
  bump.
