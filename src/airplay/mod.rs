pub mod crypto;
pub mod display;
pub mod event;
pub mod info;
pub mod mdns;
pub mod ntp;
pub mod pairing;
pub mod rtsp;
pub mod stream;

use crate::features::FrameBus;
use event::EventServer;
use mdns::MdnsAdvertiser;
use ntp::NtpServer;
use rtsp::RtspServer;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::Mutex;
use tracing::info;

pub struct ReceiverConfig {
    pub name: String,
    pub port: u16,
    pub record: bool,
    pub obs_virtual_camera: bool,
    pub pip_mode: bool,
    pub rtmp_url: Option<String>,
}

pub struct SessionState {
    pub keypair: ed25519_dalek::SigningKey,
    pub session_key: Option<[u8; 32]>,
    pub video_data_port: u16,
    pub event_port: u16,
    pub ntp_port: u16,
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
            ntp_port: 7010,
            frame_bus,
        }
    }
}

pub type SharedState = Arc<Mutex<SessionState>>;

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

        let frame_bus = FrameBus::new();
        let state: SharedState = Arc::new(Mutex::new(SessionState::new(frame_bus.clone())));

        // ─── mDNS ────────────────────────────────────────────────
        let advertiser = MdnsAdvertiser::new(&self.config.name, self.config.port)?;
        tokio::spawn(async move { advertiser.run().await });

        // ─── NTP time sync ───────────────────────────────────────
        let ntp_port = state.lock().await.ntp_port;
        let ntp = NtpServer::new(ntp_port);
        tokio::spawn(async move {
            if let Err(e) = ntp.run().await {
                tracing::error!(error = %e, "NTP server error");
            }
        });

        // ─── Event channel ───────────────────────────────────────
        let event_port = state.lock().await.event_port;
        let event = EventServer::new(event_port);
        tokio::spawn(async move {
            if let Err(e) = event.run().await {
                tracing::error!(error = %e, "Event server error");
            }
        });

        // ─── RTSP server ─────────────────────────────────────────
        let rtsp: RtspServer = RtspServer::bind(
            (Ipv4Addr::UNSPECIFIED, self.config.port),
            state.clone(),
        )
        .await?;
        tokio::spawn(async move {
            if let Err(e) = rtsp.run().await {
                tracing::error!(error = %e, "RTSP server error");
            }
        });

        // ─── Video stream receiver ───────────────────────────────
        let video_port = state.lock().await.video_data_port;
        let stream_state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = stream::listen_video(video_port, stream_state).await {
                tracing::error!(error = %e, "Video stream error");
            }
        });

        // ─── Display window ─────────────────────────────────────
        let display_bus = frame_bus.clone();
        let pip = self.config.pip_mode;
        let display_handle = std::thread::spawn(move || {
            display::run_display(display_bus.subscribe(), pip);
        });

        // ─── Recording ──────────────────────────────────────────
        if self.config.record {
            let rx = frame_bus.subscribe();
            tokio::spawn(async move {
                crate::features::recording::run(rx).await;
            });
            info!("Recording enabled → ./recordings/");
        }

        // ─── Notification capture ────────────────────────────────
        let notif_bus = frame_bus.clone();
        tokio::spawn(async move {
            crate::features::notification_capture::run(notif_bus).await;
        });

        // ─── OBS virtual camera ──────────────────────────────────
        if self.config.obs_virtual_camera {
            let rx = frame_bus.subscribe();
            tokio::spawn(async move {
                crate::features::streaming::obs_virtual_camera(rx).await;
            });
        }

        // ─── RTMP streaming ─────────────────────────────────────
        if let Some(ref url) = self.config.rtmp_url {
            let rx = frame_bus.subscribe();
            let url = url.clone();
            tokio::spawn(async move {
                crate::features::streaming::rtmp_stream(rx, url).await;
            });
        }

        info!(
            "All systems ready:\n  \
             RTSP     :7000\n  \
             NTP      :7010\n  \
             Video    :7100\n  \
             Event    :7200\n  \
             Hotkeys  : S=screenshot, Q/Esc=quit\n  \
             Waiting for iPhone..."
        );

        signal::ctrl_c().await?;
        info!("Shutting down...");
        let _ = display_handle.join();
        Ok(())
    }
}
