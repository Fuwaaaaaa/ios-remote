use bytes::BytesMut;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tracing::{debug, info, warn};

/// Event channel for AirPlay session keep-alive and feedback.
///
/// After SETUP, the iPhone opens a TCP connection to the event port.
/// It sends periodic keep-alive messages and receives feedback
/// (e.g., display size changes, input events).
pub struct EventServer {
    port: u16,
}

impl EventServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(("0.0.0.0", self.port)).await?;
        info!(port = self.port, "Event channel listening");

        loop {
            let (mut stream, peer) = listener.accept().await?;
            info!(peer = %peer, "Event channel connected");

            tokio::spawn(async move {
                let mut buf = BytesMut::with_capacity(4096);
                loop {
                    match stream.read_buf(&mut buf).await {
                        Ok(0) => {
                            info!(peer = %peer, "Event channel closed");
                            break;
                        }
                        Ok(n) => {
                            debug!(peer = %peer, bytes = n, "Event data received");
                            // Parse event messages (binary plist)
                            // For now, acknowledge and clear buffer
                            buf.clear();
                        }
                        Err(e) => {
                            warn!(peer = %peer, error = %e, "Event channel error");
                            break;
                        }
                    }
                }
            });
        }
    }
}
