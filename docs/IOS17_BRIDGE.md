# iOS 17+ bridge — staging plan & on-device check-list

This document tracks the iOS 17+ work that the legacy
`usbmuxd → lockdownd → screenshotr` path can no longer reach. It is the
authoritative status page; if it disagrees with `README.md` or
`CHANGELOG.md`, this document is correct.

The work is split into nine stages (C-1 .. C-9). Stages that can be
exercised offline (compile, lint, unit-test, run `--diag` against zero
devices) are marked **offline-verifiable**. Stages that require an iOS
17+ device on USB are marked **hardware-blocked**.

## Status (v0.7.2 — 2026-05-01)

| Stage | What it does | Status | Verifiable offline? |
|-------|--------------|--------|---------------------|
| **C-1** | `--features ios17` cargo flag pulls in `idevice = "=0.1.58"` with `usbmuxd + tcp + aws-lc + screenshotr` | ✅ Done | Yes — `cargo check --features ios17` |
| **C-2** | `usb::idevice_bridge::IdeviceBridge` adapter mirroring `LockdownClient` shape | ✅ Done | Yes — compiles + clippy clean |
| **C-3** | `connect_by_udid` runs Pair record fetch + `StartSession` + TLS upgrade via the `idevice` crate | ✅ Done | Partial — fails-fast unit test for no-usbmuxd / bogus UDID; full path = hardware |
| **C-4** | `CoreDeviceProxy` / `RemoteXPC` tunnel (the `idevice` crate's `tunneld` feature) | ⛔ Stub | No — needs RemoteXPC handshake with a real iOS 17+ device |
| **C-5** | Personalized Developer Disk Image mount via `mobile_image_mounter` | ⛔ Stub | No — needs the device's build identity / nonce |
| **C-6** | Runtime version routing: `screen_capture::capture_loop` calls `idevice_bridge::run_v2` for iOS 17+ | ✅ Done | Yes — `is_ios17_plus` unit test pins the threshold |
| **C-7** | TLS-wrapped `screenshotr` frame capture loop (actual pixel pipeline) | ⛔ Stub | No — needs the screenshotr port handed out by `start_service`, which on iOS 17+ requires C-4 + C-5 |
| **C-8** | `--diag` gains an "iOS 17+ idevice bridge probe" section reporting each step OK / FAILED | ✅ Done | Yes — `diag::run` unit test exercises the no-device path |
| **C-9** | Tests + docs cover the offline-verifiable surface | ✅ Done (this release) | Yes — `cargo test` + `cargo test --features ios17` |

**Offline-completable in v0.7.2**: C-1, C-2, C-3 (compile + scaffold),
C-6, C-8, C-9.
**Blocked on real iOS 17+ hardware (out of scope for v0.7.2)**: C-4,
C-5, C-7.

The blocked stages are gated by genuine protocol-level state that only
exists on a real device. They are *not* held back by missing tests —
they are held back by missing protocol implementations that cannot be
written meaningfully without a target to test against.

## Source map

| Concern | File | Symbol |
|---------|------|--------|
| Feature flag + dep pin | `Cargo.toml` | `[features] ios17`, `[dependencies] idevice` |
| Bridge adapter | `src/usb/idevice_bridge.rs` | `IdeviceBridge::connect_by_udid`, `device_info`, `start_service` |
| Bridge entry point | `src/usb/idevice_bridge.rs` | `run_v2(dev_info, frame_bus)` — Stage C-6 dispatch target |
| Version routing | `src/usb/screen_capture.rs` | `is_ios17_plus`, `capture_loop` `cfg(feature = "ios17")` branch |
| Major version parser | `src/usb/lockdown.rs` | `parse_ios_major` |
| `--diag` + bridge probe | `src/usb/diag.rs` | `run`, `diag_idevice_bridge` |

## Offline verification (`v0.7.2`)

All commands run on Windows with the standard MSVC toolchain. None
require an iPhone or `usbmuxd` to be running.

```powershell
# Default-build health (iOS ≤16 path)
cargo check
cargo clippy -- -D warnings
cargo test

# `ios17` preview build health (bridge compiles + tests run)
cargo check --features ios17
cargo clippy --features ios17 -- -D warnings
cargo test --features ios17

# Final release link (proves aws-lc bundles cleanly on MSVC)
cargo build --release --features ios17

# CLI surface (no device — must exit 0 with usbmuxd-FAILED or
# "No iPhone connected." printed to stdout)
cargo run -- --help
cargo run -- --diag
cargo run --features ios17 -- --diag
```

Pass-conditions: zero warnings, zero test failures, `--diag` exits with
status 0 in every host state (no usbmuxd, usbmuxd + no device, usbmuxd
+ device).

## Stage B — on-device check-list (post-v0.7.2)

This is the next step once an iOS 17+ device is available. **Do not run
these against a production device** — Stage B writes nothing, but the
diagnostic dump contains UDIDs and host-paired-device metadata.

1. Pair the iPhone with the host running `ios-remote` (open *iTunes* /
   *Apple Devices* once, accept "Trust" on the iPhone).
2. Build the preview binary:

   ```powershell
   cargo build --release --features ios17
   ```

3. Run `--diag` with stdout + stderr captured:

   ```powershell
   cargo run --release --features ios17 -- --diag > diag_ios17.txt 2>&1
   ```

4. Capture the following lines from `diag_ios17.txt` and attach them to
   the Stage B issue:

   - The `--- device <udid> ---` block (legacy lockdownd `GetValue`
     output — confirms `ProductVersion` ≥ 17).
   - `bridge connect_by_udid` line — `OK` or `FAILED — <error chain>`.
   - `bridge device_info (GetValue/TLS)` line.
   - Both `start_service` lines (`screenshotr` and `dtservicehub`) —
     `(port=…, ssl=…)` on success, full anyhow context on failure.
   - The two placeholder lines for DDI mount and `tunneld` /
     CoreDeviceProxy status (these always print today).

5. The `start_service` results decide what to build next:
   - **`start_service('com.apple.mobile.screenshotr')` succeeds** —
     skip C-4/C-5 entirely and go straight to **C-7** (TLS frame
     capture loop). The bridge already reaches the screenshotr port.
   - **`start_service('com.apple.mobile.screenshotr')` fails with
     "InvalidService" or similar** — implement **C-4** (CoreDeviceProxy
     tunnel) first, then **C-5** (Personalized DDI mount), then **C-7**.
     This is the most likely outcome on a fresh iOS 17+ device with
     Developer Mode disabled.
   - **`start_service('com.apple.instruments.dtservicehub')` succeeds**
     — the bridge can reach instrument services, which is useful but
     orthogonal to mirroring; record the result for future input
     work.

6. File the captured output in the Stage B issue along with the device
   model, iOS version (full triplet), and whether Developer Mode is
   enabled in **Settings ▸ Privacy & Security**.
