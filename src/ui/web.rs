use axum::response::Html;

/// Serve the embedded web dashboard.
///
/// A single-page app that shows real-time status, controls, and settings.
/// Served at http://localhost:8080/
pub async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="utf-8">
<title>ios-remote Dashboard</title>
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

function log(msg) {
  const el = document.getElementById('log');
  const ts = new Date().toLocaleTimeString();
  el.innerHTML += `<div>[${ts}] ${msg}</div>`;
  el.scrollTop = el.scrollHeight;
}

async function fetchStats() {
  try {
    const r = await fetch(`${API}/api/stats`);
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
  const r = await fetch(`${API}/api/screenshot`, {method:'POST'});
  const j = await r.json();
  log(j.path ? `Saved: ${j.path}` : `Error: ${j.error || 'no frame'}`);
}

async function startRec() {
  const r = await fetch(`${API}/api/recording/start`, {method:'POST'});
  log('Recording started');
}

async function stopRec() {
  const r = await fetch(`${API}/api/recording/stop`, {method:'POST'});
  log('Recording stopped');
}

async function runOcr() {
  log('Running OCR...');
  const r = await fetch(`${API}/api/ocr`, {method:'POST'});
  const j = await r.json();
  log(j.text ? `Text: ${j.text.substring(0,200)}` : `Error: ${j.error}`);
}

async function aiDescribe() {
  log('AI analyzing screen...');
  const r = await fetch(`${API}/api/ai/describe`, {method:'POST',headers:{'Content-Type':'application/json'},body:'{}'});
  const j = await r.json();
  log(j.description ? `AI: ${j.description.substring(0,300)}` : `Error: ${j.error}`);
}

async function loadHistory() {
  try {
    const r = await fetch(`${API}/api/history`);
    const h = await r.json();
    const el = document.getElementById('history');
    if (!h.records?.length) { el.textContent = 'No connections yet.'; return; }
    el.innerHTML = h.records.map(r =>
      `<div class="config-item"><span>${r.device_name}</span><span>${r.connect_count}x | ${r.total_duration_secs}s total</span></div>`
    ).join('');
  } catch(e) { document.getElementById('history').textContent = 'Error loading history.'; }
}

setInterval(fetchStats, 1000);
loadHistory();
log('Dashboard loaded');
</script>
</body>
</html>"#;
