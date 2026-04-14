pub mod crypto;
pub mod display;
pub mod info;
pub mod mdns;
pub mod pairing;
pub mod rtsp;
pub mod stream;

use mdns::MdnsAdvertiser;
use rtsp::RtspServer;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::Mutex;
use tracing::info;

/// Shared state across RTSP connections.
pub struct SessionState {
    /// Ed25519 long-term keypair for this receiver.
    pub keypair: ed25519_dalek::SigningKey,
    /// Derived session encryption key after pair-verify.
    pub session_key: Option<[u8; 32]>,
    /// Video data port assigned during SETUP.
    pub video_data_port: u16,
    /// Event port for keep-alive / feedback.
    pub event_port: u16,
}

impl SessionState {
    fn new() -> Self {
        let mut rng = rand::thread_rng();
        Self {
            keypair: ed25519_dalek::SigningKey::generate(&mut rng),
            session_key: None,
            video_data_port: 7100,
            event_port: 7200,
        }
    }
}

pub type SharedState = Arc<Mutex<SessionState>>;

/// Top-level AirPlay receiver.
pub struct AirPlayReceiver {
    name: String,
    port: u16,
}

impl AirPlayReceiver {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            port: 7000,
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        info!(name = %self.name, port = self.port, "Starting AirPlay receiver");

        let state: SharedState = Arc::new(Mutex::new(SessionState::new()));

        // mDNS: advertise _airplay._tcp so iPhones can discover us
        let advertiser = MdnsAdvertiser::new(&self.name, self.port)?;
        let mdns_handle = tokio::spawn(async move {
            advertiser.run().await;
        });

        // RTSP server: handle AirPlay negotiation
        let rtsp = RtspServer::bind((Ipv4Addr::UNSPECIFIED, self.port), state.clone()).await?;
        let rtsp_handle = tokio::spawn(async move {
            if let Err(e) = rtsp.run().await {
                tracing::error!(error = %e, "RTSP server error");
            }
        });

        // Video stream listener
        let video_port = state.lock().await.video_data_port;
        let display_state = state.clone();
        let video_handle = tokio::spawn(async move {
            if let Err(e) = stream::listen_video(video_port, display_state).await {
                tracing::error!(error = %e, "Video stream error");
            }
        });

        info!("AirPlay receiver running — waiting for iPhone connection. Ctrl+C to stop.");
        signal::ctrl_c().await?;
        info!("Shutting down...");

        mdns_handle.abort();
        rtsp_handle.abort();
        video_handle.abort();

        Ok(())
    }
}
