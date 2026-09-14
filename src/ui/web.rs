use crate::ui::api::ApiState;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::header::HOST,
    response::Html,
};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

/// Serve the embedded web dashboard.
///
/// `/` sits outside the bearer middleware (the page has to load before it can
/// authenticate), so the API token is only inlined for requests that are
/// provably from this machine: a loopback peer *and* a loopback `Host` header.
/// Everyone else — LAN clients under `--lan`, or a DNS-rebinding page whose
/// `Host` is a foreign name resolving to 127.0.0.1 — gets the page without a
/// token and is asked to paste it in.
pub async fn dashboard(State(state): State<Arc<ApiState>>, req: Request) -> Html<String> {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0);
    let host = req.headers().get(HOST).and_then(|v| v.to_str().ok());
    let token = if is_trusted_local_request(peer, host) {
        state.api_token.as_str()
    } else {
        ""
    };
    // JSON-escape the token defensively; our generator emits alphanumerics + `-_`
    // so this is belt-and-suspenders.
    let token_js = serde_json::to_string(token).unwrap_or_else(|_| "\"\"".to_string());
    let bootstrap = format!("<script>window.__IOS_REMOTE_TOKEN={token_js};</script>");
    Html(DASHBOARD_HTML.replace("<!--BOOTSTRAP-->", &bootstrap))
}

/// True only when the TCP peer is loopback and the `Host` header names a
/// loopback host. Missing connection info (e.g. a router built without
/// `into_make_service_with_connect_info`) is treated as untrusted.
pub(crate) fn is_trusted_local_request(peer: Option<SocketAddr>, host: Option<&str>) -> bool {
    let Some(peer) = peer else {
        return false;
    };
    if !peer.ip().to_canonical().is_loopback() {
        return false;
    }
    host.is_some_and(host_is_loopback)
}

fn host_is_loopback(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        // "[::1]:8080" / "[::1]"
        Some(rest) => rest.split(']').next().unwrap_or(""),
        // "127.0.0.1:8080" / "localhost" — strip an optional port.
        None => host.rsplit_once(':').map_or(host, |(h, _)| h),
    };
    name.eq_ignore_ascii_case("localhost")
        || name
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.to_canonical().is_loopback())
}

const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="utf-8">
<title>ios-remote Dashboard</title>
<!--BOOTSTRAP-->
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: -apple-system, 'Segoe UI', sans-serif; background: #1a1a2e; color: #eee; }
  .container { max-width: 900px; margin: 0 auto; padding: 20px; }
  h1 { font-size: 24px; margin-bottom: 20px; color: #00d4ff; }
  .card { background: #16213e; border-radius: 12px; padding: 20px; margin-bottom: 16px; }
  .card h2 { font-size: 16px; color: #888; margin-bottom: 12px; text-transform: uppercase; letter-spacing: 1px; }
  .stats { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
  .stat { text-align: center; }
  .stat-value { font-size: 36px; font-weight: bold; color: #00d4ff; }
  .stat-label { font-size: 12px; color: #666; margin-top: 4px; }
  .status-dot { display: inline-block; width: 10px; height: 10px; border-radius: 50%; margin-right: 8px; }
  .status-dot.on { background: #00ff88; }
  .status-dot.off { background: #ff4444; }
  .btn { background: #0f3460; border: 1px solid #00d4ff33; color: #00d4ff; padding: 10px 20px;
         border-radius: 8px; cursor: pointer; font-size: 14px; margin: 4px; transition: 0.2s; }
  .btn:hover { background: #1a4a8a; }
  .btn.active { background: #00d4ff; color: #000; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
  #log { background: #0a0a1a; border-radius: 8px; padding: 12px; font-family: monospace;
         font-size: 12px; max-height: 200px; overflow-y: auto; color: #0f0; }
  .config-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .config-item { display: flex; justify-content: space-between; padding: 8px; background: #0f3460;
                 border-radius: 6px; }
</style>
</head>
<body>
<div class="container">
  <h1>ios-remote Dashboard</h1>

  <div class="card" id="token-card" hidden>
    <h2>API token required</h2>
    <p style="font-size:13px; color:#aaa; margin-bottom:10px;">
      This page was opened from another host, so the token is not embedded.
      Paste the value of <code>[network] api_token</code> from <code>ios-remote.toml</code>.
    </p>
    <div style="display:flex; gap:8px;">
      <input id="token-input" type="password" autocomplete="off" style="flex:1; background:#0f3460; color:#eee;
             border:1px solid #00d4ff33; border-radius:6px; padding:8px;">
      <button class="btn" onclick="saveToken()">Use token</button>
    </div>
  </div>

  <div class="card">
    <h2>Status</h2>
    <div id="status-line" style="font-size:18px;">
      <span class="status-dot off" id="dot"></span>
      <span id="status-text">Waiting for iPhone...</span>
    </div>
    <div class="stats" style="margin-top:16px;">
      <div class="stat"><div class="stat-value" id="fps">--</div><div class="stat-label">FPS</div></div>
      <div class="stat"><div class="stat-value" id="frames">0</div><div class="stat-label">Frames</div></div>
      <div class="stat"><div class="stat-value" id="uptime">0s</div><div class="stat-label">Uptime</div></div>
    </div>
  </div>

  <div class="card">
    <h2>Actions</h2>
    <div class="actions">
      <button class="btn" onclick="screenshot()">Screenshot</button>
      <button class="btn" onclick="startRec()">Start Recording</button>
      <button class="btn" onclick="stopRec()">Stop Recording</button>
      <button class="btn" onclick="runOcr()">OCR (Extract Text)</button>
      <button class="btn" onclick="aiDescribe()">AI Describe Screen</button>
    </div>
  </div>

  <div class="card">
    <h2>Replay</h2>
    <div style="display:flex; gap:8px; flex-wrap:wrap; align-items:center; margin-bottom:8px;">
      <select id="replay-select" style="flex:1; min-width:200px; background:#0f3460; color:#eee;
              border:1px solid #00d4ff33; border-radius:6px; padding:8px;"></select>
      <button class="btn" onclick="replayRefresh()">Refresh</button>
      <button class="btn" onclick="replayLoad()">Load</button>
    </div>
    <div id="replay-header" style="font-size:12px; color:#888; margin-bottom:8px;">No session loaded.</div>
    <div class="actions">
      <button class="btn" onclick="replayPlay()">Play</button>
      <button class="btn" onclick="replayPause()">Pause</button>
    </div>
    <div style="margin-top:12px;">
      <input id="replay-seek" type="range" min="0" max="100" value="0" step="1" style="width:100%;"
             onchange="replaySeek(this.value)">
    </div>
    <div id="replay-bookmarks" style="display:flex; gap:4px; flex-wrap:wrap; margin-top:8px;"></div>
  </div>

  <div class="card">
    <h2>Log</h2>
    <div id="log"></div>
  </div>

  <div class="card">
    <h2>Connection History</h2>
    <div id="history">Loading...</div>
  </div>
</div>

<script>
const API = '';
const TOKEN_KEY = 'iosRemoteToken';
function readToken() {
  if (window.__IOS_REMOTE_TOKEN) return window.__IOS_REMOTE_TOKEN;
  // `#token=...` never reaches the server (fragments are client-side only).
  const m = location.hash.match(/token=([^&]+)/);
  if (m) {
    const t = decodeURIComponent(m[1]);
    try { sessionStorage.setItem(TOKEN_KEY, t); } catch(e) {}
    history.replaceState(null, '', location.pathname + location.search);
    return t;
  }
  try { return sessionStorage.getItem(TOKEN_KEY) || ''; } catch(e) { return ''; }
}
let TOKEN = readToken();
function showTokenPrompt() { document.getElementById('token-card').hidden = false; }
function saveToken() {
  TOKEN = document.getElementById('token-input').value.trim();
  try { sessionStorage.setItem(TOKEN_KEY, TOKEN); } catch(e) {}
  document.getElementById('token-card').hidden = true;
  fetchStats(); loadHistory(); replayRefresh();
}
async function api(path, opts) {
  opts = opts || {};
  const auth = TOKEN ? {'Authorization': `Bearer ${TOKEN}`} : {};
  opts.headers = Object.assign({}, opts.headers || {}, auth);
  const r = await fetch(`${API}${path}`, opts);
  if (r.status === 401) showTokenPrompt();
  return r;
}
if (!TOKEN) showTokenPrompt();

function log(msg) {
  const el = document.getElementById('log');
  const ts = new Date().toLocaleTimeString();
  el.innerHTML += `<div>[${ts}] ${msg}</div>`;
  el.scrollTop = el.scrollHeight;
}

async function fetchStats() {
  try {
    const r = await api(`/api/stats`);
    const s = await r.json();
    document.getElementById('fps').textContent = s.fps?.toFixed(1) || '--';
    document.getElementById('frames').textContent = s.frames_received || 0;
    document.getElementById('uptime').textContent = `${s.uptime_secs || 0}s`;
    const dot = document.getElementById('dot');
    const txt = document.getElementById('status-text');
    if (s.connected) {
      dot.className = 'status-dot on';
      txt.textContent = `Connected: ${s.device_name} (${s.resolution})`;
    } else {
      dot.className = 'status-dot off';
      txt.textContent = 'Waiting for iPhone...';
    }
  } catch(e) {}
}

async function screenshot() {
  log('Taking screenshot...');
  const r = await api(`/api/screenshot`, {method:'POST'});
  const j = await r.json();
  log(j.path ? `Saved: ${j.path}` : `Error: ${j.error || 'no frame'}`);
}

async function startRec() {
  const r = await api(`/api/recording/start`, {method:'POST'});
  log('Recording started');
}

async function stopRec() {
  const r = await api(`/api/recording/stop`, {method:'POST'});
  log('Recording stopped');
}

async function runOcr() {
  log('Running OCR...');
  const r = await api(`/api/ocr`, {method:'POST'});
  const j = await r.json();
  log(j.text ? `Text: ${j.text.substring(0,200)}` : `Error: ${j.error}`);
}

async function aiDescribe() {
  log('AI analyzing screen...');
  const r = await api(`/api/ai/describe`, {method:'POST',headers:{'Content-Type':'application/json'},body:'{}'});
  const j = await r.json();
  log(j.description ? `AI: ${j.description.substring(0,300)}` : `Error: ${j.error}`);
}

async function loadHistory() {
  try {
    const r = await api(`/api/history`);
    const h = await r.json();
    const el = document.getElementById('history');
    if (!h.records?.length) { el.textContent = 'No connections yet.'; return; }
    el.innerHTML = h.records.map(r =>
      `<div class="config-item"><span>${r.device_name}</span><span>${r.connect_count}x | ${r.total_duration_secs}s total</span></div>`
    ).join('');
  } catch(e) { document.getElementById('history').textContent = 'Error loading history.'; }
}

let replayDurationUs = 0;

async function replayRefresh() {
  try {
    const r = await api(`/api/replay/sessions`);
    const j = await r.json();
    const sel = document.getElementById('replay-select');
    sel.innerHTML = '';
    (j.sessions || []).forEach(s => {
      const opt = document.createElement('option');
      opt.value = s.path;
      const name = s.path.split(/[\\/]/).pop();
      opt.textContent = `${name} (${s.total_frames}f, ${s.duration_secs.toFixed(1)}s)`;
      sel.appendChild(opt);
    });
    if (!j.sessions?.length) {
      const opt = document.createElement('option');
      opt.textContent = 'No sessions under ./recordings';
      opt.disabled = true;
      sel.appendChild(opt);
    }
  } catch(e) { log(`Replay refresh error: ${e}`); }
}

async function replayLoad() {
  const path = document.getElementById('replay-select').value;
  if (!path) { log('No session selected'); return; }
  const r = await api(`/api/replay/load`, {
    method:'POST',
    headers:{'Content-Type':'application/json'},
    body: JSON.stringify({ path })
  });
  const j = await r.json();
  if (j.error) { log(`Replay load failed: ${j.error}`); return; }
  const h = j.header || {};
  replayDurationUs = Math.round((h.duration_secs || 0) * 1_000_000);
  document.getElementById('replay-header').textContent =
    `${h.width}×${h.height}, ${h.total_frames} frames, ${(h.duration_secs || 0).toFixed(1)}s`;
  const bm = document.getElementById('replay-bookmarks');
  bm.innerHTML = '';
  (j.bookmarks || []).forEach(b => {
    const btn = document.createElement('button');
    btn.className = 'btn';
    btn.textContent = b.label || `@${Math.round(b.timestamp_us/1000)}ms`;
    btn.onclick = () => replaySeekUs(b.timestamp_us);
    bm.appendChild(btn);
  });
  log(`Loaded: ${path}`);
}

async function replayPlay() {
  const r = await api(`/api/replay/play`, {method:'POST'});
  const j = await r.json();
  log(j.status === 'playing' ? 'Replay playing' : `Replay play: ${j.error || j.status}`);
}

async function replayPause() {
  await api(`/api/replay/pause`, {method:'POST'});
  log('Replay paused');
}

async function replaySeek(pct) {
  if (!replayDurationUs) { log('Load a session first'); return; }
  const ts_us = Math.round((pct / 100) * replayDurationUs);
  await replaySeekUs(ts_us);
}

async function replaySeekUs(ts_us) {
  const r = await api(`/api/replay/seek`, {
    method:'POST',
    headers:{'Content-Type':'application/json'},
    body: JSON.stringify({ ts_us })
  });
  const j = await r.json();
  log(j.status === 'seeked' ? `Seek → NAL ${j.position}` : `Seek: ${j.error || j.status}`);
}

setInterval(fetchStats, 1000);
loadHistory();
replayRefresh();
log('Dashboard loaded');
</script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::is_trusted_local_request;
    use std::net::SocketAddr;

    fn peer(s: &str) -> Option<SocketAddr> {
        Some(s.parse().unwrap())
    }

    #[test]
    fn loopback_peer_with_loopback_host_is_trusted() {
        assert!(is_trusted_local_request(
            peer("127.0.0.1:50000"),
            Some("127.0.0.1:8080")
        ));
        assert!(is_trusted_local_request(
            peer("127.0.0.1:50000"),
            Some("localhost:8080")
        ));
        assert!(is_trusted_local_request(
            peer("[::1]:50000"),
            Some("[::1]:8080")
        ));
        assert!(is_trusted_local_request(
            peer("[::ffff:127.0.0.1]:50000"),
            Some("127.0.0.1")
        ));
    }

    #[test]
    fn lan_peer_never_gets_the_token() {
        // `--lan`: another host hits 0.0.0.0:8080 using this machine's LAN IP
        // or even a spoofed loopback Host header.
        assert!(!is_trusted_local_request(
            peer("192.168.1.20:50000"),
            Some("192.168.1.10:8080")
        ));
        assert!(!is_trusted_local_request(
            peer("192.168.1.20:50000"),
            Some("127.0.0.1:8080")
        ));
    }

    #[test]
    fn dns_rebinding_host_is_rejected() {
        // Browser resolved evil.example → 127.0.0.1; peer is loopback but the
        // Host header still names the attacker's domain.
        assert!(!is_trusted_local_request(
            peer("127.0.0.1:50000"),
            Some("evil.example:8080")
        ));
        assert!(!is_trusted_local_request(
            peer("127.0.0.1:50000"),
            Some("localhost.evil.example")
        ));
    }

    #[test]
    fn missing_connect_info_or_host_is_untrusted() {
        assert!(!is_trusted_local_request(None, Some("127.0.0.1:8080")));
        assert!(!is_trusted_local_request(peer("127.0.0.1:50000"), None));
    }
}
