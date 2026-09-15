# Security

## Reporting vulnerabilities

Please report security issues privately to the maintainer rather than filing a
public issue. For research-only or low-severity reports, a regular issue is
fine.

## Fixed vulnerabilities

| Affected | Fixed in | Issue |
|----------|----------|-------|
| ≤ 0.8.0 | 0.8.1 | **API token disclosure via `GET /`.** The dashboard page is served outside the bearer middleware and embedded the API token for every caller. With `--lan` (or `network.lan_access = true`), any host that could reach the port could read the token and then call every `/api/*` endpoint — overwrite the config, toggle the Windows startup registry entry, quit the app, run macros. Default loopback-only installs were reachable only from the same machine. **Upgrade to 0.8.1 and rotate the token** (clear `[network] api_token` in `ios-remote.toml` or set `IOS_REMOTE_API_TOKEN`) if you ever ran with `--lan`. |

## Dependency audit — v0.8.1

`cargo audit --deny warnings` (RustSec advisory-db, 1246 advisories loaded)
against `Cargo.lock` with 421 crate dependencies:

- **Vulnerabilities:** 0
- **Warnings:** 0

CI runs the same check on every push and pull request (`cargo audit` job in
`.github/workflows/test.yml`; soft-fail so new advisories surface without
blocking unrelated merges). v0.8.1 updated `crossbeam-epoch`, `plist`
(→ `quick-xml`), `rustls`, `anyhow`, `rqrr` (→ `lru`) and `chacha20` to
clear the advisories that appeared after v0.8.0 — see CHANGELOG.

## Runtime security posture

- Default bind address is `127.0.0.1`. LAN exposure requires the explicit
  `--lan` flag or `network.lan_access = true` in `ios-remote.toml`.
- Every `/api/*` route requires a Bearer token (32-byte URL-safe random,
  generated on first launch, persisted to the config file, constant-time
  compare on each request).
- The dashboard page (`/`) embeds the token only for requests whose TCP peer
  is loopback **and** whose `Host` header is a loopback name or address;
  LAN clients and DNS-rebinding pages get a page that asks for the token.
- Outbound HTTP goes through `curl -K -`: API keys and request bodies never
  appear on a process command line.
- No `unwrap()` / `expect()` in `src/` on the default build (clippy-denied).
- `build.rs` rejects non-Windows targets so the intended runtime environment
  is encoded in the build itself.
