# Security

## Reporting vulnerabilities

Please report security issues privately to the maintainer rather than filing a
public issue. For research-only or low-severity reports, a regular issue is
fine.

## Dependency audit — v0.5.0

`cargo audit` (RustSec advisory-db, 1049 advisories loaded) against
`Cargo.lock` with 357 transitive deps:

- **Vulnerabilities:** 0
- **Warnings:** 3 — all upstream / transitive, none directly exploitable from
  ios-remote's code paths.

| Advisory | Crate | Via | Status |
|----------|-------|-----|--------|
| RUSTSEC-2024-0384 | `instant 0.1.13` (unmaintained) | `minifb 0.28` | Waiting on minifb to drop the dep |
| RUSTSEC-2024-0436 | `paste 1.0.15` (unmaintained) | `rav1e → ravif → image` | Proc-macro only; no runtime code path |
| RUSTSEC-2026-0002 | `lru 0.12.5` (unsound) | `rqrr 0.8.0` | `IterMut` edge; we don't call it |

These are "warnings", not "errors", in the advisory-db taxonomy. None of them
opens an attack surface against ios-remote's own binary. Revisit when any of
minifb / image / rqrr publishes an update that drops the affected crate.

## Runtime security posture

- Default bind address is `127.0.0.1`. LAN exposure requires the explicit
  `--lan` flag or `network.lan_access = true` in `ios-remote.toml`.
- Every `/api/*` route requires a Bearer token (32-byte URL-safe random,
  generated on first launch, persisted to the config file, constant-time
  compare on each request).
- No `unwrap()` / `expect()` in `src/` on the default build (clippy-denied).
- `build.rs` rejects non-Windows targets so the intended runtime environment
  is encoded in the build itself.
