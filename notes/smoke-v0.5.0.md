# Smoke Test — v0.5.0

**Date:** 2026-04-21
**Binary:** `target/release/ios-remote.exe` (v0.5.0, re-built after PR9–12)
**Tested at:** Post-release tag `v0.5.0`, commit `ec07d1a`
**Tester:** agent-driven API/auth smoke (no iPhone), then human completion

This file is the running record for Phase A of the v0.5.0 release plan.
Update each section with PASS / FAIL / log excerpts as you work through it.

## Environment

| Item | Value |
|------|-------|
| OS | Windows 11 (harness) |
| Rust | stable (via dtolnay/rust-toolchain) |
| iTunes / Apple Devices installed | **NO** on this machine (expected; that's why Steps 2–8 are deferred to the human) |
| AppleMobileDeviceService running | N/A (not installed) |
| iPhone attached | **NO** on this machine |

---

## Agent-run scenarios (iPhone not required)

### ✅ S1. Binary produces the right help text
```
$ ios-remote.exe --help
iPhone screen mirroring via USB Type-C (Windows only)
...
      --lan              Expose the Web Dashboard / API on 0.0.0.0 ...
      --bind <BIND>      Override the bind address ...
      --token <TOKEN>    Override API token ...
      --device <DEVICE>  Select a specific iPhone by UDID ...
      --list-devices     Print the connected iPhone list and exit
```
**Pass.** All PR3 flags present. Version string reports `v0.5.0`.

### ✅ S2. `--list-devices` without usbmuxd
```
$ ios-remote.exe --list-devices
INFO ios-remote v0.5.0 — USB Type-C mode
Error: Cannot connect to usbmuxd (127.0.0.1:27015). Is iTunes or Apple Devices
       app installed? Error: 対象のコンピューターによって拒否されたため、接続できませんでした。
       (os error 10061)
Exit code 1
```
**Pass.** Graceful diagnostic — user is pointed at the fix (install Apple Devices)
instead of getting a raw backtrace. Exit code is non-zero as expected.

### ✅ S3. Web Dashboard launches on first run (no iPhone)
```
INFO ios-remote v0.5.0 — USB Type-C mode
INFO ios_remote::config: Default configuration created file="ios-remote.toml"
INFO API token (Bearer): vllAFbEnIBaz-32_IFamGjvT3NTx3DLP
INFO Local-only mode — use --lan to expose on all interfaces. bind=127.0.0.1:18080
INFO Web dashboard: http://127.0.0.1:18080
```
**Pass.** Config auto-created, token generated+logged, local-only message shown.

### ✅ S4. Token authentication
```
GET /                                         → HTTP 200   dashboard HTML (token inlined)
GET /api/stats (no auth)                      → HTTP 401   ✓ bearer middleware rejects
GET /api/stats (wrong token)                  → HTTP 401   ✓ wrong token rejected
GET /api/stats (correct Bearer)               → HTTP 200   ✓ accepted, JSON body returned
GET /api/stats (?token=…)                     → HTTP 200   ✓ query-string fallback works
```
Response body (truncated):
```
{"connected":false,"device_name":"","fps":0.0,"frames_received":0,
 "uptime_secs":0,"resolution":"","bitrate_kbps":0.0}
```
**Pass.** All four auth paths behave as specified.

### ✅ S4b. Recording API end-to-end
```
POST /api/recording/start              → {"path":"recordings\\rec_…h264","status":"recording_started"}
POST /api/recording/start (again)      → {"error":"recording already in progress","status":"error"}
POST /api/recording/stop               → {"path":"recordings\\rec_…h264","status":"recording_stopped"}
POST /api/recording/stop (idle)        → {"error":"no recording in progress","status":"idle"}
```
File `recordings/rec_20260421_134004_214398.h264` created on disk.
**Pass.** RecordingController single-flight guard + path reporting both work.

### ✅ S5. USB reconnect backoff (no device)
Logs after ~2s of running:
```
WARN ios_remote::usb: USB session ended — will retry
     error=Cannot connect to usbmuxd (127.0.0.1:27015)...
```
**Pass.** Instead of crashing, the receiver logs the error and will retry with
exponential backoff (1→2→4→8→16s). Display window continues to exist.
The "still waiting" 30s periodic warning was not hit in this short test; verify
manually by leaving the binary running for >30s with no device.

### ✅ S6. Display window appears
`INFO ios_remote::features::display: Display window opened pip=false`

**Pass.** minifb window spawns even without a connected iPhone.

### ✅ S7. Config round-trip to disk
After the short smoke run, `ios-remote.toml` exists in CWD with the generated
token and the new `network` section fields (`bind_address = "127.0.0.1"`,
`lan_access = false`, `api_token = "..."`).
**Pass.** Cleanup removed the file after the test.

### ✅ S8. Unit + integration tests
`cargo test` → **17 passed, 0 failed** (9 unit + 8 integration).
**Pass.**

---

## Human-required scenarios (iPhone + iTunes/Apple Devices needed)

The following require a real iPhone and the Apple Mobile Device Service running
on the Windows box. Please run `target/release/ios-remote.exe` with iPhone
connected and fill in results here.

### ⏳ H1. usbmuxd → lockdownd → screenshotr chain
Expected log lines on successful connection:
- `Connected to usbmuxd at 127.0.0.1:27015`
- `Connected to iPhone` with UDID + device_id
- `Starting screen capture via USB...`
- First decoded frame appears in the display window within ~1 second

- [ ] Pass / Fail:
- iOS version:
- iPhone model:
- Notes / logs:

### ⏳ H2. Screenshot via API
```
curl -H "Authorization: Bearer <TOKEN>" -X POST http://127.0.0.1:8080/api/screenshot
```
Expected: JSON `{"path": "screenshots/..."}` and a real PNG on disk.

- [ ] Pass / Fail:
- File path saved:
- File size (should be > 10 KB):

### ⏳ H3. Recording start / stop
```
curl -H "Authorization: Bearer <TOKEN>" -X POST http://127.0.0.1:8080/api/recording/start
# ... wait 5 seconds ...
curl -H "Authorization: Bearer <TOKEN>" -X POST http://127.0.0.1:8080/api/recording/stop
```
Expected: response contains `"path": "recordings/rec_YYYYMMDD_HHMMSS.h264"`, file
exists with non-zero size, ffprobe can read it.

- [ ] Pass / Fail:
- File size:
- ffprobe output:

### ⏳ H4. USB reconnect (physical unplug)
1. Connect iPhone, see frames flowing
2. Unplug USB cable
3. Observe `Device disconnected` or reconnect warnings, display window goes dark
4. Plug back in → log shows device found again, frames resume

- [ ] Pass / Fail:
- Reconnect time (seconds):
- Logs:

### ⏳ H5. Multi-device (two iPhones)
1. Connect two iPhones
2. `ios-remote.exe --list-devices` shows both UDIDs
3. `ios-remote.exe --device <UDID-of-phone-2>` mirrors phone 2 specifically
4. Without `--device`, the log shows `Multiple devices connected — using first`

- [ ] Pass / Fail (can skip if only one iPhone available):
- UDIDs seen:

### ⏳ H6. LAN exposure (`--lan`)
1. Start with `--lan` flag
2. Log shows `LAN access enabled — ... Keep the token secret.` warning
3. From another PC on the same LAN: `curl -H "Authorization: Bearer <TOKEN>" http://<host-ip>:8080/api/stats` → 200
4. Without token → 401
5. Stop, restart without `--lan`, retry from other PC → connection refused

- [ ] Pass / Fail:
- Tested from IP:

### ⏳ H7. WDA macros (only if WebDriverAgent is installed on iPhone)
1. Install WDA, run it on iPhone, forward port 8100
2. Create `macros/test.json` with a Tap action
3. `curl -H "Authorization: Bearer <TOKEN>" -X POST -d '{"name":"test"}' http://127.0.0.1:8080/api/macros/run`
4. Observe tap on iPhone screen

- [ ] Pass / Fail:
- Without WDA running: does the call return an error without panic? (expected)

---

## Ship readiness

Once all H* items above are Pass (or explicitly skipped with reason), run:

```bash
git tag -a v0.5.0 -m "v0.5.0 — USB-only, safety hardening, replay + WDA"
git push origin master
git push origin v0.5.0
```

The `release.yml` workflow will build `ios-remote-v0.5.0-windows-x64.zip` and
create a GitHub Release using CHANGELOG.md as the body.

If any H* fails with a blocker, open a fix PR first, re-run S* agent scenarios
to confirm no regression, then retry H*.
