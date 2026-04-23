use super::{Frame, FrameBus};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Live H.264 encoder: subscribes to the FrameBus, feeds each RGBA frame into
/// an `ffmpeg`/`libx264` subprocess, and republishes the resulting NAL units
/// back onto the bus so recording / RTMP / SessionRecorder consumers see
/// populated `Frame.h264_nalu`. Without this, the screenshotr PNG→RGBA path
/// produces no H.264 and every H.264-only consumer is a no-op.
///
/// Loopback protection: frames we publish carry an empty `rgba` and a filled
/// `h264_nalu`; the encoder skips both conditions on input so it never feeds
/// its own output back through ffmpeg.
pub struct H264Encoder {
    frame_bus: FrameBus,
    ffmpeg_bin: String,
}

impl H264Encoder {
    pub fn new(frame_bus: FrameBus) -> Self {
        Self {
            frame_bus,
            ffmpeg_bin: "ffmpeg".to_string(),
        }
    }

    /// Test seam: point at a nonexistent binary to verify graceful fallback.
    #[must_use]
    pub fn with_ffmpeg_bin(mut self, bin: impl Into<String>) -> Self {
        self.ffmpeg_bin = bin.into();
        self
    }

    /// Fire-and-forget: spawn the encoder loop as a background task.
    pub fn spawn(self) {
        tokio::spawn(async move { self.run().await });
    }

    async fn run(self) {
        let mut rx = self.frame_bus.subscribe();
        let mut current_dims: Option<(u32, u32)> = None;
        let mut child: Option<tokio::process::Child> = None;
        let mut stdin: Option<tokio::process::ChildStdin> = None;
        let mut reader_handle: Option<tokio::task::JoinHandle<()>> = None;
        let mut warned_missing = false;

        loop {
            match rx.recv().await {
                Ok(frame) => {
                    // Skip our own h264-only output and any rgba-less feed.
                    if frame.rgba.is_empty() || frame.h264_nalu.is_some() {
                        continue;
                    }

                    let dims = (frame.width, frame.height);
                    let needs_spawn = current_dims != Some(dims) || child.is_none();
                    if needs_spawn {
                        teardown(&mut child, &mut stdin, &mut reader_handle).await;
                        match spawn_encoder(&self.ffmpeg_bin, dims.0, dims.1) {
                            Ok((c, sin, sout)) => {
                                child = Some(c);
                                stdin = Some(sin);
                                let bus_for_reader = self.frame_bus.clone();
                                reader_handle = Some(tokio::spawn(reader_task(
                                    sout,
                                    bus_for_reader,
                                    dims.0,
                                    dims.1,
                                )));
                                current_dims = Some(dims);
                                info!(
                                    width = dims.0,
                                    height = dims.1,
                                    "H.264 encoder started (libx264 / ultrafast / zerolatency)"
                                );
                            }
                            Err(e) => {
                                if !warned_missing {
                                    warn!(
                                        error = %e,
                                        "Failed to spawn ffmpeg encoder — recording / replay / RTMP will carry no H.264 payload"
                                    );
                                    warned_missing = true;
                                }
                                continue;
                            }
                        }
                    }

                    if let Some(sin) = stdin.as_mut()
                        && sin.write_all(&frame.rgba).await.is_err()
                    {
                        warn!("encoder stdin write failed; will respawn on next frame");
                        teardown(&mut child, &mut stdin, &mut reader_handle).await;
                        current_dims = None;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(dropped = n, "Encoder falling behind — dropped frames");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }

        teardown(&mut child, &mut stdin, &mut reader_handle).await;
    }
}

async fn teardown(
    child: &mut Option<tokio::process::Child>,
    stdin: &mut Option<tokio::process::ChildStdin>,
    reader_handle: &mut Option<tokio::task::JoinHandle<()>>,
) {
    // Drop stdin first so ffmpeg sees EOF and flushes before we kill.
    drop(stdin.take());
    if let Some(mut c) = child.take() {
        let _ = c.kill().await;
    }
    if let Some(h) = reader_handle.take() {
        h.abort();
    }
}

fn spawn_encoder(
    bin: &str,
    width: u32,
    height: u32,
) -> Result<
    (
        tokio::process::Child,
        tokio::process::ChildStdin,
        tokio::process::ChildStdout,
    ),
    String,
> {
    let size = format!("{width}x{height}");
    let mut child = tokio::process::Command::new(bin)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &size,
            "-r",
            "30",
            "-i",
            "pipe:0",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-tune",
            "zerolatency",
            "-f",
            "h264",
            "pipe:1",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("spawn ffmpeg: {e}"))?;
    let stdin = child.stdin.take().ok_or("ffmpeg stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("ffmpeg stdout unavailable")?;
    Ok((child, stdin, stdout))
}

async fn reader_task(
    mut stdout: tokio::process::ChildStdout,
    frame_bus: FrameBus,
    width: u32,
    height: u32,
) {
    let mut pending: Vec<u8> = Vec::with_capacity(64 * 1024);
    let mut chunk = vec![0u8; 16 * 1024];
    loop {
        match stdout.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                pending.extend_from_slice(&chunk[..n]);
                for nal in split_annex_b_streaming(&mut pending) {
                    let timestamp_us = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_micros() as u64)
                        .unwrap_or(0);
                    frame_bus.publish(Frame {
                        width,
                        height,
                        rgba: Vec::new(),
                        timestamp_us,
                        h264_nalu: Some(nal),
                    });
                }
            }
            Err(_) => break,
        }
    }
}

/// Drain all *complete* Annex-B NAL units from `buf`, leaving a trailing
/// partial NAL (plus its start code) at the front of `buf` for the next call.
/// "Complete" means a subsequent start code has been seen — callers must
/// accept that the very last NAL before EOF will be dropped here. libx264's
/// output is always followed by more start codes while the encoder is alive,
/// so in practice the only lost NAL is the tail at shutdown.
pub(crate) fn split_annex_b_streaming(buf: &mut Vec<u8>) -> Vec<Vec<u8>> {
    // Positions of start codes in `buf`, stored as (offset, code_length).
    let mut code_positions: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i + 3 <= buf.len() {
        if buf[i] == 0 && buf[i + 1] == 0 {
            if i + 4 <= buf.len() && buf[i + 2] == 0 && buf[i + 3] == 1 {
                code_positions.push((i, 4));
                i += 4;
                continue;
            }
            if buf[i + 2] == 1 {
                code_positions.push((i, 3));
                i += 3;
                continue;
            }
        }
        i += 1;
    }

    if code_positions.len() < 2 {
        // Either zero start codes (garbage at head) or exactly one (first NAL
        // is forming, not yet terminated). Keep everything for the next call.
        return Vec::new();
    }

    let mut nals = Vec::with_capacity(code_positions.len() - 1);
    for win in code_positions.windows(2) {
        let (code_start, code_len) = win[0];
        let nal_data_start = code_start + code_len;
        let nal_data_end = win[1].0;
        nals.push(buf[nal_data_start..nal_data_end].to_vec());
    }

    let Some(&(last_code_start, _)) = code_positions.last() else {
        return nals;
    };
    // Retain the last start code and its (still-forming) NAL payload so the
    // next scan can pair it with the subsequent start code.
    let tail: Vec<u8> = buf[last_code_start..].to_vec();
    *buf = tail;
    nals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_annex_b_streaming_extracts_complete_nals() {
        let mut buf = vec![
            0x00, 0x00, 0x00, 0x01, 0xAA, // NAL 1 payload AA
            0x00, 0x00, 0x00, 0x01, 0xBB, 0xCC, // NAL 2 payload BB CC
            0x00, 0x00, 0x00, 0x01, 0xDD, // NAL 3 forming, not terminated
        ];
        let nals = split_annex_b_streaming(&mut buf);
        assert_eq!(nals, vec![vec![0xAA], vec![0xBB, 0xCC]]);
        assert_eq!(buf, vec![0x00, 0x00, 0x00, 0x01, 0xDD]);
    }

    #[test]
    fn split_annex_b_streaming_handles_split_start_code() {
        // First chunk: partial start code prefix only.
        let mut buf = vec![0x00, 0x00];
        assert!(split_annex_b_streaming(&mut buf).is_empty());
        assert_eq!(buf, vec![0x00, 0x00]);

        // Second chunk: completes the first start code, one NAL, another
        // start code for the next NAL.
        buf.extend_from_slice(&[0x00, 0x01, 0xAA, 0x00, 0x00, 0x00, 0x01, 0xBB]);
        let nals = split_annex_b_streaming(&mut buf);
        assert_eq!(nals, vec![vec![0xAA]]);
        // What remains is the second start code + the forming NAL payload.
        assert_eq!(buf, vec![0x00, 0x00, 0x00, 0x01, 0xBB]);
    }

    #[test]
    fn split_annex_b_streaming_no_emit_until_next_start_code() {
        let mut buf = vec![0x00, 0x00, 0x00, 0x01, 0xAA, 0xBB];
        let nals = split_annex_b_streaming(&mut buf);
        assert!(
            nals.is_empty(),
            "single NAL must not be emitted until the next start code arrives"
        );
        assert_eq!(buf, vec![0x00, 0x00, 0x00, 0x01, 0xAA, 0xBB]);
    }

    #[test]
    fn split_annex_b_streaming_accepts_mixed_3_and_4_byte_start_codes() {
        let mut buf = vec![
            0x00, 0x00, 0x01, 0xAA, // 3-byte start code + NAL AA
            0x00, 0x00, 0x00, 0x01, 0xBB, // 4-byte start code + NAL BB (forming)
        ];
        let nals = split_annex_b_streaming(&mut buf);
        assert_eq!(nals, vec![vec![0xAA]]);
        assert_eq!(buf, vec![0x00, 0x00, 0x00, 0x01, 0xBB]);
    }

    #[tokio::test]
    #[ignore]
    async fn encoder_emits_h264_with_real_ffmpeg() {
        let bus = FrameBus::new();
        let mut rx = bus.subscribe();
        H264Encoder::new(bus.clone()).spawn();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let width: u32 = 64;
        let height: u32 = 64;
        let pixel_count = (width * height) as usize;
        let rgba: Vec<u8> = [0xFFu8, 0x00, 0x00, 0xFF].repeat(pixel_count);

        let pub_bus = bus.clone();
        let pub_rgba = rgba.clone();
        tokio::spawn(async move {
            for i in 0..60u64 {
                pub_bus.publish(Frame {
                    width,
                    height,
                    rgba: pub_rgba.clone(),
                    timestamp_us: i * 33_000,
                    h264_nalu: None,
                });
                tokio::time::sleep(std::time::Duration::from_millis(33)).await;
            }
        });

        let result = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                match rx.recv().await {
                    Ok(frame) if frame.h264_nalu.is_some() => return true,
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return false,
                }
            }
        })
        .await;

        assert!(
            result.unwrap_or(false),
            "encoder did not produce any h264 NALs within 3s"
        );
    }
}
