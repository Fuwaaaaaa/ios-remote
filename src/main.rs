mod airplay;
mod error;

use airplay::AirPlayReceiver;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("ios_remote=debug".parse()?)
                .add_directive("info".parse()?),
        )
        .init();

    let receiver = AirPlayReceiver::new("ios-remote");
    receiver.run().await
}
