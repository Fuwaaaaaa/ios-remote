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

### P1 — Documentation honesty (from the 2026-09-14 hands-on audit)

- **[P1] README feature lists overstate what ships**
  About 52 of the ~80 feature / system / devtools modules the README
  advertises are never reached from a startup path, and most documented
  hotkeys are not bound. Mark each feature as working / experimental /
  not wired, or drop it. v0.8.1 only corrected the sections whose
  behavior changed in that release.

- **[P1] Real-device capture has never been verified**
  No smoke run has recorded a real iPhone showing frames. The default USB
  path starts `screenshotr` without StartSession / TLS / a mounted
  Developer Disk Image. Tracked by the v0.9 hardware-first rebuild
  (`tools/ios-probe`); until then the README should not present USB
  mirroring as working.

### P3 — CI hygiene (2026-05-25)

- **[P3] Bump `KyleMayes/install-llvm-action@v2.0.9` → `@v3` when published**
  Currently pinned to v2.0.9. Upstream has no node24 release yet
  (all v2.0.x use node20; still true on 2026-09-15). GitHub Actions
  force-migrates to node24 on 2026-06-02 and removes node20 on
  2026-09-16 — if the `whisper` job starts failing to set up LLVM after
  that, this is why (the job passed on 2026-09-15). Recheck the repo's
  releases monthly; bump when `v3.x.y` ships and remove the version pin.
  See `.github/workflows/test.yml:68`. Not used by `release.yml`.

- **[P3] Optionally pin `windows-latest` → `windows-2025`**
  GitHub will redirect `windows-latest` to `windows-2025-vs2026` on
  2026-06-15. The redirect is automatic and harmless for our build, but
  an explicit pin makes runner choice deterministic and surfaces future
  image upgrades as intentional changes rather than silent drift.
  Affects `.github/workflows/test.yml:12,51` and
  `.github/workflows/release.yml:14`.

### P3 — Future / iOS 17+ track (carried from CHANGELOG "Deferred")

- **[P3] Stage C-7 — TLS-wrapped screenshotr capture loop on iOS 17+**
  Requires real iOS 17+ hardware. Synthetic mode (v0.8.0) is the dev/demo
  fallback while this is blocked.

- **[P3] Phase C deferred commands** (from v0.6 CHANGELOG)
  `annotation_*`, `ruler`, `privacy_*` — pending args signature bump.

---

## Completed

- **[2026-09-15] v0.8.1 security + audit fixes** — `5032f89` (#11): API
  token no longer leaks from `GET /`; real HTTP status codes; image-only
  latest frame; measured stats + connection history; partial configs load
  and broken ones are never overwritten; replayable recordings; shared
  curl client (AI vision on real screenshots, secrets off argv). See
  CHANGELOG `[0.8.1]`.

- **[2026-09-15] cargo audit clean + racy `wda_client` tests** —
  `f28fad9` (#12): `crossbeam-epoch`, `plist`/`quick-xml`, `rustls`,
  `anyhow`, `rqrr`/`lru`, `chacha20`; the two env-var tests now share one
  lock.

- **[2026-09-15] Feature branch + PR for releases** — v0.8.1 shipped
  through PRs (#11, #12, and the release PR) instead of direct-to-master.
  (`/ship` itself was not adopted.)

- **[2026-05-29] Interactive Synthetic Mode** — `--synthetic` now drives a
  shared `DeviceState` from WDA input (tap opens apps, swipe flips pages,
  home/back returns), the macro engine's `Repeat` + `WaitForScreen` are
  implemented, and `GET /api/synthetic/state` exposes a read-only view.
  Folded in the v0.8.0 `/review` follow-ups below. Shipped in
  CHANGELOG `[0.8.1]`.

- **[2026-05-29] I1 — `SyntheticHandles` Drop semantics** — `impl Drop`
  now aborts all three spawned tasks (`src/synthetic/mod.rs`).

- **[2026-05-29] I2 — Renderer underflow on small `width`** —
  `saturating_sub` in the status bar + `layout::grid_left`; guarded by
  `narrow_device_does_not_panic`.

- **[2026-05-29] I3 — WDA stub log injection** — handlers log only parsed
  integer coordinates, never the raw body (`src/synthetic/wda_stub.rs`).

- **[2026-05-29] I5 — E2E subprocess stdio** — captured to a per-test temp
  log, dumped when `IOS_REMOTE_E2E_LOGS=1`.

- **[2026-05-29] I6 — E2E coverage gaps** — added drag + long-press via
  session, `--synthetic --diag` fast-exit, and subtitle rotation across
  ticks; plus interactivity + macro-pipeline e2e.

- **[2026-05-29] I7 — Cross-platform synthetic** — decided Windows-only
  (display layer is Windows-gated by `build.rs`); README "CI runners"
  wording clarified to mean Windows runners.

- **[2026-05-29] WDA port-collision regression test** — pre-binds the WDA
  port, spawns `--synthetic`, asserts non-zero exit (locks in the C2 fix).

- **[2026-05-29] cargo fmt clean-up** — `src/usb/diag.rs` +
  `src/usb/idevice_bridge.rs` reformatted; `cargo fmt --check` is clean.

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
