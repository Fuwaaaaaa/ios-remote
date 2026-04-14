pub mod crypto;
pub mod display;
pub mod info;
pub mod mdns;
pub mod pairing;
pub mod rtsp;
pub mod stream;

use crate::features::FrameBus;
use mdns::MdnsAdvertiser;
use rtsp::RtspServer;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::Mutex;
use tracing::info;

/// Configuration from CLI arguments.
pub struct ReceiverConfig {
    pub name: String,
    pub port: u16,
    pub record: bool,
    pub obs_virtual_camera: bool,
    pub pip_mode: bool,
    pub rtmp_url: Option<String>,
}

/// Shared state across RTSP connections.
pub struct SessionState {
    pub keypair: ed25519_dalek::SigningKey,
    pub session_key: Option<[u8; 32]>,
    pub video_data_port: u16,
    pub event_port: u16,
    /// Bus for distributing decoded frames to all consumers.
    pub frame_bus: FrameBus,
}

impl SessionState {
    fn new(frame_bus: FrameBus) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            keypair: ed25519_dalek::SigningKey::generate(&mut rng),
            session_key: None,
            video_data_port: 7100,
            event_port: 7200,
            frame_bus,
        }
    }
}

pub type SharedState = Arc<Mutex<SessionState>>;

/// Top-level AirPlay receiver with all features.
pub struct AirPlayReceiver {
    config: ReceiverConfig,
}

impl AirPlayReceiver {
    pub fn new(config: ReceiverConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        info!(
            name = %self.config.name,
            port = self.config.port,
            record = self.config.record,
            obs = self.config.obs_virtual_camera,
            pip = self.config.pip_mode,
            "Starting ios-remote"
        );

        // Frame bus: decoded frames are broadcast to all consumers
        // (display, recorder, screenshot, OBS, RTMP, AI, etc.)
        let frame_bus = FrameBus::new();
        let state: SharedState = Arc::new(Mutex::new(SessionState::new(frame_bus.clone())));

        // ─── mDNS ────────────────────────────────────────────────
        let advertiser = MdnsAdvertiser::new(&self.config.name, self.config.port)?;
        let mdns_handle = tokio::spawn(async move {
            advertiser.run().await;
        });

        // ─── RTSP server ─────────────────────────────────────────
        let rtsp: RtspServer = RtspServer::bind(
            (Ipv4Addr::UNSPECIFIED, self.config.port),
            state.clone(),
        )
        .await?;
        let rtsp_handle = tokio::spawn(async move {
            if let Err(e) = rtsp.run().await {
                tracing::error!(error = %e, "RTSP server error");
            }
        });

        // ─── Video stream receiver ───────────────────────────────
        let video_port = state.lock().await.video_data_port;
        let stream_state = state.clone();
        let video_handle = tokio::spawn(async move {
            if let Err(e) = stream::listen_video(video_port, stream_state).await {
                tracing::error!(error = %e, "Video stream error");
            }
        });

        // ─── Feature consumers (each subscribes to frame_bus) ────

        // Display window (runs on OS thread, not tokio)
        let display_bus = frame_bus.clone();
        let pip = self.config.pip_mode;
        let display_handle = std::thread::spawn(move || {
            display::run_display(display_bus.subscribe(), pip);
        });

        // Recording
        if self.config.record {
            let rec_bus = frame_bus.clone();
            tokio::spawn(async move {
                crate::features::recording::run(rec_bus.subscribe()).await;
            });
            info!("Recording enabled → ./recordings/");
        }

        // Screenshot listener (Ctrl+S hotkey in display window)
        let ss_bus = frame_bus.clone();
        tokio::spawn(async move {
            crate::features::screenshot::run(ss_bus.subscribe()).await;
        });

        // OBS virtual camera
        if self.config.obs_virtual_camera {
            info!("OBS virtual camera: ready (pipe: \\\\.\\pipe\\ios-remote-cam)");
        }

        // RTMP streaming
        if let Some(ref url) = self.config.rtmp_url {
            info!(url = %url, "RTMP streaming: ready");
        }

        info!("All systems ready. Waiting for iPhone. Ctrl+C to stop.");
        signal::ctrl_c().await?;
        info!("Shutting down...");

        mdns_handle.abort();
        rtsp_handle.abort();
        video_handle.abort();
        let _ = display_handle.join();

        Ok(())
    }
}
