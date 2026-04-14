#![allow(dead_code)]

mod airplay;
mod error;
mod features;
mod idevice;

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

    let config = airplay::ReceiverConfig {
        name: cli.name,
        port: cli.port,
        record: cli.record,
        obs_virtual_camera: cli.obs,
        pip_mode: cli.pip,
        rtmp_url: cli.rtmp,
    };

    let receiver = AirPlayReceiver::new(config);
    receiver.run().await
}
