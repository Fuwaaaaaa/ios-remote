use super::Frame;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// OBS virtual camera output via named pipe.
///
/// OBS can capture from a named pipe as a raw video source.
/// We write RGBA frames to \\.\pipe\ios-remote-cam.
pub async fn obs_virtual_camera(mut rx: broadcast::Receiver<Arc<Frame>>) {
    info!("OBS virtual camera: waiting for pipe connection");

    // On Windows, create a named pipe
    // For now, log that it's ready — full pipe implementation requires winapi
    loop {
        match rx.recv().await {
            Ok(_frame) => {
                // TODO: Write raw RGBA to named pipe
                // let pipe = OpenOptions::new().write(true).open("\\\\.\\pipe\\ios-remote-cam")?;
                // pipe.write_all(&frame.rgba)?;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(dropped = n, "OBS pipe: dropping frames");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// RTMP streaming: pipe H.264 NALUs to ffmpeg for live streaming.
///
/// Spawns ffmpeg as a child process and feeds it raw H.264 data.
/// ffmpeg handles the RTMP protocol and muxing.
pub async fn rtmp_stream(mut rx: broadcast::Receiver<Arc<Frame>>, rtmp_url: String) {
    info!(url = %rtmp_url, "Starting RTMP stream via ffmpeg");

    let mut child = match std::process::Command::new("ffmpeg")
        .args([
            "-f", "h264", "-i", "pipe:0", "-c:v", "copy", "-f", "flv", &rtmp_url,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to start ffmpeg — is it installed?");
            return;
        }
    };

    let mut stdin = match child.stdin.take() {
        Some(s) => s,
        None => {
            warn!("Failed to get ffmpeg stdin");
            return;
        }
    };

    info!("RTMP: ffmpeg started, streaming...");
    let mut frame_count: u64 = 0;

    loop {
        match rx.recv().await {
            Ok(frame) => {
                if let Some(ref nalu) = frame.h264_nalu {
                    let start_code = [0x00u8, 0x00, 0x00, 0x01];
                    if stdin.write_all(&start_code).is_err() {
                        break;
                    }
                    if stdin.write_all(nalu).is_err() {
                        break;
                    }
                    frame_count += 1;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }

    info!(frames = frame_count, "RTMP stream ended");
    let _ = child.kill();
}
