#![allow(dead_code)]

mod airplay;
mod config;
mod devtools;
mod error;
mod features;
mod idevice;
mod system;
mod ui;

use airplay::AirPlayReceiver;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "ios-remote", about = "AirPlay mirroring receiver + iPhone integration")]
struct Cli {
    /// Receiver display name (shown on iPhone)
    #[arg(short, long, default_value = "ios-remote")]
    name: String,

    /// RTSP listen port
    #[arg(short, long, default_value_t = 7000)]
    port: u16,

    /// Web dashboard port
    #[arg(short = 'w', long, default_value_t = 8080)]
    web_port: u16,

    /// Enable recording (saves to ./recordings/)
    #[arg(long)]
    record: bool,

    /// Enable OBS virtual camera output
    #[arg(long)]
    obs: bool,

    /// Enable always-on-top picture-in-picture mode
    #[arg(long)]
    pip: bool,

    /// Enable RTMP streaming (provide URL)
    #[arg(long)]
    rtmp: Option<String>,

    /// Use config file instead of CLI args
    #[arg(long)]
    config: bool,
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

    // Load or create config file if --config flag is set
    let app_config = if cli.config {
        config::AppConfig::load()
    } else {
        config::AppConfig::default()
    };

    let receiver_config = airplay::ReceiverConfig {
        name: if cli.config { app_config.receiver.name.clone() } else { cli.name },
        port: if cli.config { app_config.receiver.port } else { cli.port },
        record: cli.record || app_config.recording.auto_record,
        obs_virtual_camera: cli.obs || app_config.features.obs_virtual_camera,
        pip_mode: cli.pip || app_config.display.pip_mode,
        rtmp_url: if cli.rtmp.is_some() { cli.rtmp } else if !app_config.network.rtmp_url.is_empty() { Some(app_config.network.rtmp_url.clone()) } else { None },
    };

    // Start web dashboard + REST API
    let frame_bus = features::FrameBus::new();
    let api_state = std::sync::Arc::new(ui::api::ApiState {
        frame_bus: frame_bus.clone(),
        config: std::sync::Arc::new(tokio::sync::Mutex::new(app_config)),
        history: std::sync::Arc::new(tokio::sync::Mutex::new(config::ConnectionHistory::load())),
        stats: std::sync::Arc::new(tokio::sync::Mutex::new(ui::api::StreamStats::default())),
    });

    let web_port = cli.web_port;
    let api = api_state.clone();
    tokio::spawn(async move {
        let app = ui::api::router(api)
            .route("/", axum::routing::get(ui::web::dashboard));

        let listener = tokio::net::TcpListener::bind(("0.0.0.0", web_port))
            .await
            .expect("Failed to bind web dashboard port");

        tracing::info!(port = web_port, "Web dashboard: http://localhost:{}", web_port);
        axum::serve(listener, app).await.unwrap();
    });

    let receiver = AirPlayReceiver::new(receiver_config);
    receiver.run().await
}
