use super::Frame;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// VR overlay: display iPhone screen as a floating panel in VR space.
///
/// Uses OpenVR (SteamVR) overlay API to render frames as a texture
/// on a virtual panel that floats in front of the user.
///
/// Requires SteamVR to be running.
pub struct VrOverlay {
    width: f32,
    distance: f32,
}

impl VrOverlay {
    pub fn new() -> Self {
        Self {
            width: 0.5,     // 50cm wide panel
            distance: 1.5,  // 1.5m in front
        }
    }

    /// Run the VR overlay loop. Blocks the calling thread.
    pub fn run(&self, mut rx: broadcast::Receiver<Arc<Frame>>) {
        info!("VR overlay: initializing OpenVR...");

        // OpenVR initialization
        // In a full implementation, this would:
        // 1. Call vr::init(ApplicationType::Overlay)
        // 2. Create an overlay with IVROverlay::CreateOverlay
        // 3. Set overlay width and transform
        // 4. On each frame: upload RGBA texture → SetOverlayTexture

        // For now, log readiness. Full OpenVR integration requires
        // the `openvr` crate and SteamVR runtime.

        #[cfg(feature = "vr")]
        {
            // TODO: openvr::init() and overlay creation
            unimplemented!("VR overlay requires 'vr' feature flag and SteamVR");
        }

        #[cfg(not(feature = "vr"))]
        {
            info!("VR overlay: compiled without 'vr' feature. \
                   Add `openvr` to Cargo.toml and enable 'vr' feature to use.");

            // Drain frames to avoid broadcast lag
            loop {
                match rx.blocking_recv() {
                    Ok(_) => {} // would upload to VR overlay
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(dropped = n, "VR overlay lagging");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}
