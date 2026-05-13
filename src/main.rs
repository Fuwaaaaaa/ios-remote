#![allow(dead_code)]
// Pixel-drawing helpers (rgba: &mut [u8], w, h, x, y, color, ...) naturally
// exceed clippy's default 7-argument threshold. Grouping them into a struct
// would obscure hot-path call sites without meaningful benefit.
#![allow(clippy::too_many_arguments)]
// PR2 removed all panicking unwraps; keep new ones out of the tree. Tests
// are exempt because assertions with unwrap are idiomatic for propagating
// test failure information.
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod config;
mod devtools;
mod error;
mod features;
mod idevice;
mod synthetic;
mod system;
mod ui;
mod usb;

use clap::Parser;
use features::FrameBus;
use std::net::{IpAddr, SocketAddr};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "ios-remote",
    about = "iPhone screen mirroring via USB Type-C (Windows only)"
)]
struct Cli {
    /// Display window name
    #[arg(short, long, default_value = "ios-remote")]
    name: String,

    /// Web dashboard port
    #[arg(short = 'w', long, default_value_t = 8080)]
    web_port: u16,

    /// Enable recording
    #[arg(long)]
    record: bool,

    /// PiP mode (always on top)
    #[arg(long)]
    pip: bool,

    /// Expose the Web Dashboard / API on 0.0.0.0 (LAN). An API token is required
    /// for all /api/* requests regardless of this flag.
    #[arg(long)]
    lan: bool,

    /// Override the bind address (e.g. 127.0.0.1 or 192.168.1.10). When --lan is
    /// set, this flag is ignored and 0.0.0.0 is used.
    #[arg(long)]
    bind: Option<IpAddr>,

    /// Override API token (also accepted via env IOS_REMOTE_API_TOKEN). If unset,
    /// the token from config is used; if config has none, one is generated.
    #[arg(long)]
    token: Option<String>,

    /// Select a specific iPhone by UDID (see --list-devices).
    #[arg(long)]
    device: Option<String>,

    /// Print the connected iPhone list and exit.
    #[arg(long)]
    list_devices: bool,

    /// Run a one-shot diagnostic dump (usbmuxd + lockdownd + screenshotr probe)
    /// and exit. Useful for debugging "Trust" / "screen does not display"
    /// issues, especially on iOS 17+ devices.
    #[arg(long)]
    diag: bool,

    /// Synthetic device mode — run the entire pipeline without a real iPhone.
    /// Spawns an iPhone-shaped mock screen at 30 FPS, a dummy WebDriverAgent
    /// stub on 127.0.0.1:8101 (override with --synthetic-wda-port), and a
    /// rotating subtitle pump. Useful for development, demos, CI smoke
    /// tests, and as a fallback when iOS 17+ hardware is unavailable.
    /// See notes/v0.8.0.md.
    #[arg(long)]
    synthetic: bool,

    /// Override the dummy WDA stub bind port (only meaningful with
    /// --synthetic). Defaults to 8101 so the production default of 8100
    /// stays free for real iproxy-forwarded WDA.
    #[arg(long, default_value_t = 8101)]
    synthetic_wda_port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("ios_remote=debug".parse()?)
                .add_directive("info".parse()?),
        )
        .init();

    let cli = Cli::parse();

    tracing::info!(
        "ios-remote v{} — USB Type-C mode",
        env!("CARGO_PKG_VERSION")
    );

    // ── Device listing short-circuit ────────────────────────────────────────
    if cli.list_devices {
        return usb::print_device_list().await;
    }

    // ── Diagnostic short-circuit ───────────────────────────────────────────
    if cli.diag {
        return usb::diag::run().await;
    }

    // ── Config + token ──────────────────────────────────────────────────────
    let mut app_config = config::AppConfig::load();
    if cli.lan {
        app_config.network.lan_access = true;
    }
    let api_token = cli
        .token
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| app_config.resolve_api_token());

    let bind_ip: IpAddr = if app_config.network.lan_access {
        IpAddr::from([0, 0, 0, 0])
    } else if let Some(ip) = cli.bind {
        ip
    } else {
        app_config
            .network
            .bind_address
            .parse()
            .unwrap_or_else(|_| IpAddr::from([127, 0, 0, 1]))
    };

    let web_addr = SocketAddr::new(bind_ip, cli.web_port);
    tracing::info!(
        "API token (Bearer) ready: {} (full token via debug log)",
        mask_token(&api_token)
    );
    tracing::debug!(token = %api_token, "API token (Bearer) full value");
    if app_config.network.lan_access {
        tracing::warn!(
            bind = %web_addr,
            "LAN access enabled — the dashboard is reachable from other hosts. Keep the token secret."
        );
    } else {
        tracing::info!(
            bind = %web_addr,
            "Local-only mode — use --lan to expose on all interfaces."
        );
    }

    // ── Frame bus: decoded frames broadcast to all consumers ────────────────
    let frame_bus = FrameBus::new();

    // ── H.264 encoder (RGBA → H.264 on the fly; feeds recording / replay /
    //    RTMP with populated `Frame.h264_nalu`). No-op if ffmpeg is missing.
    features::h264_encoder::H264Encoder::new(frame_bus.clone()).spawn();

    // ── Recording controller (shared across CLI --record and the REST API) ──
    let recorder = features::recording::RecordingController::new(frame_bus.clone());
    if cli.record {
        match recorder.start() {
            Ok(path) => {
                tracing::info!(file = %path.display(), "Recording enabled → {}", path.display())
            }
            Err(e) => tracing::warn!(error = %e, "Could not start recording"),
        }
    }

    // ── Session replay controller (shared with the REST API) ────────────────
    let replay = features::session_replay::SessionPlaybackController::new(frame_bus.clone());

    // ── Display state (shared with dispatch handlers) ───────────────────────
    let display_state = std::sync::Arc::new(std::sync::Mutex::new(
        features::display_state::DisplayState::new(),
    ));

    // ── Audio capture + transcription (gated) ───────────────────────────────
    // The transcriber is shared between the capture pump (writes subtitles)
    // and the display loop (reads + draws them); both consult the same
    // monotonic clock through `Transcriber::now_ms()`.
    #[cfg(feature = "audio_capture")]
    let (transcriber, _audio_handle) = {
        use features::audio_capture::{AudioBus, AudioCapture, AudioSource};
        let configured = match features::audio_capture::AudioSource::parse(&app_config.audio.source)
        {
            Some(src) => src,
            None => {
                tracing::warn!(
                    raw = %app_config.audio.source,
                    "Unknown audio.source value (expected loopback / mic / off); disabling audio capture"
                );
                AudioSource::Off
            }
        };
        if configured == AudioSource::Off {
            tracing::info!("Audio capture: disabled by config");
            (
                None::<std::sync::Arc<std::sync::Mutex<features::audio_transcription::Transcriber>>>,
                None,
            )
        } else {
            let transcriber = std::sync::Arc::new(std::sync::Mutex::new(
                features::audio_transcription::Transcriber::new(),
            ));
            let bus = AudioBus::new();
            let handle = AudioCapture::new(bus.clone(), configured).spawn();
            if handle.is_some() {
                features::audio_capture::spawn_transcription_pump(
                    bus,
                    transcriber.clone(),
                    app_config.audio.chunk_secs,
                );
            }
            let transcriber_opt = if handle.is_some() {
                Some(transcriber)
            } else {
                None
            };
            (transcriber_opt, handle)
        }
    };
    #[cfg(not(feature = "audio_capture"))]
    let transcriber: Option<
        std::sync::Arc<std::sync::Mutex<features::audio_transcription::Transcriber>>,
    > = None;

    // In synthetic mode the subtitle pump needs a Transcriber even if audio
    // capture was disabled or unbuilt. Synthesizing one here keeps
    // /api/subtitles populated and the on-screen subtitle bar working.
    let transcriber = if cli.synthetic && transcriber.is_none() {
        Some(std::sync::Arc::new(std::sync::Mutex::new(
            features::audio_transcription::Transcriber::new(),
        )))
    } else {
        transcriber
    };

    // ── Display window (OS thread) ──────────────────────────────────────────
    // Spawned after recorder/replay/display_state exist so the title bar's
    // activity indicator and zoom transform can read state every frame.
    let display_bus = frame_bus.clone();
    let display_recorder = recorder.clone();
    let display_replay = replay.clone();
    let display_state_for_window = display_state.clone();
    let display_transcriber = transcriber.clone();
    let pip = cli.pip;
    let display_handle = std::thread::spawn(move || {
        features::display::run_display(
            display_bus.subscribe(),
            pip,
            display_recorder,
            display_replay,
            display_state_for_window,
            display_transcriber,
        );
    });

    // ── Shared API state ────────────────────────────────────────────────────
    // Built up-front (before the web spawn) so the Stream Deck HID thread
    // can also dispatch through it.
    let dashboard_url = if app_config.network.lan_access {
        // Browser opens locally; even with --lan we want the loopback URL on
        // this machine (the LAN form would require knowing this host's IP).
        format!("http://127.0.0.1:{}", cli.web_port)
    } else {
        format!("http://{}", web_addr)
    };
    let api_state = std::sync::Arc::new(ui::api::ApiState {
        frame_bus: frame_bus.clone(),
        config: std::sync::Arc::new(tokio::sync::Mutex::new(app_config.clone())),
        history: std::sync::Arc::new(tokio::sync::Mutex::new(config::ConnectionHistory::default())),
        stats: std::sync::Arc::new(tokio::sync::Mutex::new(ui::api::StreamStats::default())),
        api_token: api_token.clone(),
        recorder: recorder.clone(),
        replay: replay.clone(),
        dashboard_url,
        display: display_state.clone(),
        transcriber: transcriber.clone(),
    });

    // ── Web dashboard ───────────────────────────────────────────────────────
    let web_state = api_state.clone();
    tokio::spawn(async move {
        let app = ui::api::router(web_state.clone()).route(
            "/",
            axum::routing::get(ui::web::dashboard).with_state(web_state),
        );
        match tokio::net::TcpListener::bind(web_addr).await {
            Ok(listener) => {
                tracing::info!(addr = %web_addr, "Web dashboard: http://{}", web_addr);
                if let Err(e) = axum::serve(listener, app).await {
                    tracing::error!(error = %e, "Web server stopped with error");
                }
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    addr = %web_addr,
                    "Failed to bind Web dashboard — is the port already in use?"
                );
            }
        }
    });

    // ── Stream Deck HID loop (only when --features stream_deck is on) ───────
    #[cfg(feature = "stream_deck")]
    {
        let sd_state = api_state.clone();
        std::thread::spawn(move || {
            let integration = features::stream_deck::StreamDeckIntegration::new();
            features::stream_deck::run_event_loop(integration, sd_state);
        });
    }
    #[cfg(not(feature = "stream_deck"))]
    let _ = &api_state; // silence the "only used when feature is on" lint

    // ── Synthetic device mode (no iPhone required) ─────────────────────────
    if cli.synthetic {
        if cli.device.is_some() {
            tracing::warn!("--device is ignored when --synthetic is set");
        }

        let info = synthetic::SyntheticDeviceInfo::default_iphone_15();

        // Pre-populate stats so /api/status reports "connected" and the
        // dashboard's Status card shows the synthetic device identity.
        {
            let mut stats = api_state.stats.lock().await;
            stats.connected = true;
            stats.device_name = info.name.clone();
            stats.resolution = format!("{}x{}", info.width, info.height);
            stats.fps = 30.0;
        }

        // Redirect macros REST → dummy WDA on the loopback port we're about
        // to bind. This MUST happen before the stub spawn and before any
        // call into WdaClient::default_wda_client().
        let wda_addr: std::net::SocketAddr =
            std::net::SocketAddr::from(([127, 0, 0, 1], cli.synthetic_wda_port));
        // SAFETY: env::set_var is called once on the main thread before any
        // worker reads IOS_REMOTE_WDA_URL. The Rust 2024 edition marks this
        // unsafe because env reads from other threads are undefined without
        // synchronization; we guarantee the ordering by setting before spawn.
        unsafe {
            std::env::set_var(
                "IOS_REMOTE_WDA_URL",
                format!("http://127.0.0.1:{}", cli.synthetic_wda_port),
            );
        }

        // Bridge synthetic frames → existing FrameBus. Down-stream consumers
        // (display, recording, screenshot, OCR, AI, replay) are unchanged.
        let bus_for_frames = frame_bus.clone();
        let frame_publish: std::sync::Arc<
            dyn Fn(synthetic::renderer::SyntheticFrame) + Send + Sync,
        > = std::sync::Arc::new(move |sf: synthetic::renderer::SyntheticFrame| {
            bus_for_frames.publish(features::Frame {
                width: sf.width,
                height: sf.height,
                rgba: sf.rgba,
                timestamp_us: sf.timestamp_us,
                h264_nalu: None,
            });
        });

        // Bridge synthetic subtitles → existing Transcriber (always Some in
        // synthetic mode — see the override above).
        let subtitle_push: std::sync::Arc<dyn Fn(String) + Send + Sync> =
            match transcriber.clone() {
                Some(tr) => std::sync::Arc::new(move |text: String| {
                    let mut guard = tr.lock().unwrap_or_else(|e| e.into_inner());
                    let ts = guard.now_ms();
                    // Overlap consecutive lines by 1s so the bar never blanks
                    // between 5-second ticks (duration 6s, push every 5s).
                    guard.add_subtitle(&text, ts, 6_000);
                }),
                None => std::sync::Arc::new(|_| {}),
            };

        tracing::info!(
            udid = %info.udid,
            wda = %wda_addr,
            "Synthetic device mode — no iPhone required. \
             Dashboard at http://127.0.0.1:{}",
            cli.web_port
        );

        let _handles = synthetic::spawn(info, frame_publish, subtitle_push, wda_addr);

        // Wait for the display window to close (Q/Esc/X). The synthetic
        // background tasks are aborted on handle drop right after.
        let _ = display_handle.join();
        return Ok(());
    }

    // ── iproxy supervisor (auto-tunnel for WebDriverAgent macros) ───────────
    // Held for the lifetime of main; on Ctrl+C the OS reaps the child along
    // with us. Returns None silently if iproxy isn't on PATH or port 8100 is
    // already forwarded — neither is fatal.
    let _iproxy = features::iproxy_supervisor::try_spawn(cli.device.as_deref());

    // ── USB connection (main task) ──────────────────────────────────────────
    let receiver = usb::UsbReceiver::new(frame_bus).with_udid(cli.device.clone());
    receiver.run().await?;

    let _ = display_handle.join();
    Ok(())
}

/// Render the API token for human-readable startup logs without exposing the
/// full secret. Keeps the leading and trailing 4 characters so an operator
/// can still cross-reference against a saved value, but the middle is hidden.
/// The full token is only emitted at `debug` level for explicit opt-in.
fn mask_token(t: &str) -> String {
    let chars: Vec<char> = t.chars().collect();
    let n = chars.len();
    if n <= 8 {
        return "*".repeat(n);
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars.iter().skip(n - 4).collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::mask_token;

    #[test]
    fn mask_short_token_is_fully_starred() {
        assert_eq!(mask_token(""), "");
        assert_eq!(mask_token("abcd"), "****");
        assert_eq!(mask_token("12345678"), "********");
    }

    #[test]
    fn mask_long_token_keeps_head_and_tail() {
        let m = mask_token("ABCD1234567890wxyz");
        assert!(m.starts_with("ABCD"), "got {m:?}");
        assert!(m.ends_with("wxyz"), "got {m:?}");
        assert!(!m.contains("1234567890"));
    }
}
