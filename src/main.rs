#![allow(dead_code)]

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
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "ios-remote", about = "iPhone screen mirroring via USB Type-C")]
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

    // Frame bus: decoded frames broadcast to all consumers
    let frame_bus = FrameBus::new();

    // Display window (OS thread)
    let display_bus = frame_bus.clone();
    let pip = cli.pip;
    let display_handle = std::thread::spawn(move || {
        features::display::run_display(display_bus.subscribe(), pip);
    });

    // Recording
    if cli.record {
        let rx = frame_bus.subscribe();
        tokio::spawn(async move {
            features::recording::run(rx).await;
        });
        tracing::info!("Recording enabled → ./recordings/");
    }

    // Web dashboard
    let web_bus = frame_bus.clone();
    let web_port = cli.web_port;
    tokio::spawn(async move {
        let api_state = std::sync::Arc::new(ui::api::ApiState {
            frame_bus: web_bus,
            config: std::sync::Arc::new(tokio::sync::Mutex::new(config::AppConfig::default())),
            history: std::sync::Arc::new(tokio::sync::Mutex::new(config::ConnectionHistory::default())),
            stats: std::sync::Arc::new(tokio::sync::Mutex::new(ui::api::StreamStats::default())),
        });
        let app = ui::api::router(api_state)
            .route("/", axum::routing::get(ui::web::dashboard));
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", web_port)).await.unwrap();
        tracing::info!(port = web_port, "Web dashboard: http://localhost:{}", web_port);
        let _ = axum::serve(listener, app).await;
    });

    // USB connection (main task)
    let receiver = usb::UsbReceiver::new(frame_bus);
    receiver.run().await?;

    let _ = display_handle.join();
    Ok(())
}
