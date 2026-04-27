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
    tracing::info!("API token (Bearer): {token}", token = &api_token);
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

    // ── Display window (OS thread) ──────────────────────────────────────────
    // Spawned after recorder/replay/display_state exist so the title bar's
    // activity indicator and zoom transform can read state every frame.
    let display_bus = frame_bus.clone();
    let display_recorder = recorder.clone();
    let display_replay = replay.clone();
    let display_state_for_window = display_state.clone();
    let pip = cli.pip;
    let display_handle = std::thread::spawn(move || {
        features::display::run_display(
            display_bus.subscribe(),
            pip,
            display_recorder,
            display_replay,
            display_state_for_window,
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
        history: std::sync::Arc::new(tokio::sync::Mutex::new(
            config::ConnectionHistory::default(),
        )),
        stats: std::sync::Arc::new(tokio::sync::Mutex::new(ui::api::StreamStats::default())),
        api_token: api_token.clone(),
        recorder: recorder.clone(),
        replay: replay.clone(),
        dashboard_url,
        display: display_state.clone(),
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
