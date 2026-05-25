# TODOS

Backlog of items captured outside CHANGELOG.md (which only tracks shipped
changes). Format: priority label, one-line title, optional notes block.
Items move to `## Completed` when shipped, with the resolving commit SHA.

Priority scheme:
- **P0** — Production-affecting, must ship next
- **P1** — Should ship next release
- **P2** — Important but not blocking (default for /review follow-ups)
- **P3** — Nice to have / deferred

---

## Open

### P2 — v0.8.0 /review follow-ups (2026-05-25)

Captured during pre-tag `/review` of v0.8.0. None block the release; all
are quality improvements to land in v0.8.1 or later.

- **[P2] I1 — `SyntheticHandles` Drop semantics**
  `src/synthetic/mod.rs:50-54`. Dropping the handle struct does NOT abort the
  contained `tokio::task::JoinHandle`s — it only detaches them. The doc
  comment claims "Drop = abort all spawned tasks" which is false. Either
  implement `impl Drop for SyntheticHandles` that calls `.abort()` on each
  task, or rewrite the comment to match actual semantics. In practice the
  main loop returns immediately after `display_handle.join()` so the tokio
  runtime tears everything down anyway — this is correctness-of-docs more
  than a runtime bug.
  Effort: ~15 min CC.

- **[P2] I2 — Renderer underflow on `width < 22`**
  `src/synthetic/renderer.rs:121-124,141`. The status bar battery indicator
  uses `w - 22`, `w - 21`, `w - 6`; the app grid uses
  `(w - total_grid_w) / 2`. These panic with arithmetic underflow if a
  caller constructs `SyntheticDeviceInfo` with `width < 22` or
  `width < total_grid_w + 2`. `SyntheticDeviceInfo` is `pub` with public
  fields, so nothing prevents it. Fix with `saturating_sub` or a guarded
  constructor that rejects small widths. The renderer task panicking is
  silent (no `.await` on the JoinHandle), which compounds I1.
  Effort: ~15 min CC.

- **[P2] I3 — WDA stub log injection surface**
  `src/synthetic/wda_stub.rs:50-65`. The tap/drag/touchAndHold handlers
  log the entire request body via `info!(?body, ...)`. An attacker that
  can reach `127.0.0.1:8101` (local user, or any process that can resolve
  `localhost` via DNS rebinding from a browser page) can inject arbitrary
  bytes into structured tracing output. The stub binds loopback only and
  the production use case is local dev/CI, so impact is limited — but
  worth re-evaluating if logs ever feed into an indexed or alert pipeline.
  Fix: switch to a sanitized debug print of only `x`/`y`/`duration`
  fields, or escape control characters.
  Effort: ~20 min CC.

- **[P2] I5 — E2E subprocess stdio captured to `Stdio::null()`**
  `tests/synthetic_e2e.rs:40-41`. When an integration test fails on CI,
  the spawned binary's tracing output is invisible — you can only see
  HTTP response codes. Capture stderr to a `tempfile::tempfile()` per
  test and dump it on failure (or gate via env var like
  `IOS_REMOTE_E2E_LOGS=1`). Useful for triaging flaky CI without local
  repro.
  Effort: ~30 min CC.

- **[P2] I6 — E2E coverage gaps**
  Three specific holes in `tests/synthetic_e2e.rs`:
  1. `dummy_wda_drag_returns_200_via_session` — currently only unit-level
     coverage in `src/synthetic/wda_stub.rs::tests`.
  2. `dummy_wda_long_press_returns_200_via_session` — same.
  3. `--synthetic --diag` interaction — CHANGELOG promises `--diag`
     short-circuits and ignores `--synthetic`; verify by spawning with
     both flags and asserting fast exit (no web server bind).
  4. Subtitle rotation across multiple ticks — current
     `pump_pushes_first_line_immediately` only checks tick #1.
  Each is ~30-40 lines mirroring the existing tap test.
  Effort: ~45 min CC total.

- **[P2] I7 — Cross-platform synthetic compile**
  `tests/synthetic_e2e.rs:7`. The E2E suite is `#![cfg(target_os =
  "windows")]`. The synthetic source modules (`src/synthetic/*.rs`) are
  not OS-gated and likely compile on macOS/Linux, but `features::display`
  (scrap/minifb) is Windows-only per `build.rs`. README claims `--synthetic`
  is useful on "phone-less CI runners" — confirm whether that includes
  non-Windows runners or document the Windows-only constraint explicitly.
  Effort: ~30 min CC (investigate + doc update).

### P2 — Pre-existing maintenance (noticed during /review)

- **[P2] cargo fmt clean-up for `src/usb/diag.rs` + `src/usb/idevice_bridge.rs`**
  rustfmt reports diffs for these files in v0.7.2 baseline (pre-existing,
  not from v0.8.0). Bundle into next chore commit before tagging v0.8.1.
  Effort: 1 min — `cargo fmt`.

### P2 — v0.8.1 quality investments (from /retro habits)

- **[P2] Regression test for WDA port collision**
  Add an E2E test that pre-binds `8101`, then spawns `--synthetic`, and
  asserts the process exits non-zero with a clear error. Locks in the C2
  fix (`3e28740`) so a future refactor cannot silently regress to the old
  `warn! + continue` behavior.
  Effort: ~20 min CC.

- **[P2] Try `/ship` workflow for next release**
  v0.8.0 was direct-to-master. Switch to a feature branch + PR + `/ship`
  for v0.8.1 so review log, CHANGELOG bump, and tag creation are
  automated.

### P3 — Future / iOS 17+ track (carried from CHANGELOG "Deferred")

- **[P3] Stage C-7 — TLS-wrapped screenshotr capture loop on iOS 17+**
  Requires real iOS 17+ hardware. Synthetic mode (v0.8.0) is the dev/demo
  fallback while this is blocked.

- **[P3] Phase C deferred commands** (from v0.6 CHANGELOG)
  `annotation_*`, `ruler`, `privacy_*` — pending args signature bump.

---

## Completed

- **[2026-05-25] I4 — `bind_random` test race** — resolved as a
  side-effect of C2 fix (commit `3e28740`). New `serve(listener)`
  signature lets the test pass its `TcpListener` directly without the
  bind→drop→rebind dance. The 120 ms sleep was also cut to 50 ms.

- **[2026-05-25] C1 — `IOS_REMOTE_WDA_URL` env::set_var race** — commit
  `3e28740`. Moved `env::set_var` to immediately after CLI parse, before
  any tokio task spawns.

- **[2026-05-25] C2 — WDA stub bind failure silent** — commit `3e28740`.
  `wda_stub::spawn(addr)` → `wda_stub::serve(listener)`; main now binds
  synchronously and `anyhow::bail!`s on port-in-use with a hint to use
  `--synthetic-wda-port`.

- **[2026-05-25] Pre-tag housekeeping** — commit `dda8a7d`. `.gitignore`
  for `/recordings/`, `/screenshots/`, `/ios-remote.toml`; trimmed
  "(planned)" from notes/v0.8.0.md; simplified mock clock arithmetic.

- **[2026-05-13] v0.8.0 Synthetic Device Mode (Tasks 1〜10)** — see
  CHANGELOG.md for the full set. 9 commits, +1215 LOC net.
