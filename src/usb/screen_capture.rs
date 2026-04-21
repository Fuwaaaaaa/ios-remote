use super::usbmuxd::UsbmuxdClient;
use crate::features::{Frame, FrameBus};
use image::codecs::png::PngDecoder;
use image::ImageDecoder;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, warn};

/// Screen capture via USB using the screenshotr service.
///
/// The screenshotr service (com.apple.mobile.screenshotr) captures
/// individual screenshots as PNG images. We poll rapidly to create
/// a "live" mirror effect.
///
/// Protocol:
///   1. Connect to screenshotr service port via usbmuxd
///   2. Send DLMessageVersionExchange
///   3. Send DLMessageProcessMessage with ScreenShotRequest
///   4. Receive PNG image data
///   5. Repeat from step 3
const SCREENSHOTR_SERVICE: &str = "com.apple.mobile.screenshotr";

pub async fn capture_loop(
    mux: &mut UsbmuxdClient,
    device_id: u32,
    frame_bus: FrameBus,
) -> anyhow::Result<()> {
    // Start screenshotr service via lockdownd
    let mut lockdown = super::lockdown::LockdownClient::connect(mux, device_id).await?;

    let dev_info = lockdown.get_device_info().await?;
    info!(
        name = %dev_info.name,
        model = %dev_info.model,
        ios = %dev_info.ios_version,
        "Device info"
    );

    let service = lockdown.start_service(SCREENSHOTR_SERVICE).await?;

    // Connect to the screenshotr service port
    let mut ss_tunnel = UsbmuxdClient::connect().await?;
    ss_tunnel.connect_to_device(device_id, service.port).await?;
    let stream = ss_tunnel.stream_mut();

    info!(port = service.port, "Screenshotr connected — starting capture loop");

    // Version exchange
    send_version_exchange(stream).await?;
    let _ver_resp = recv_message(stream).await?;
    info!("Screenshotr version exchange complete");

    let mut frame_count = 0u64;
    let start = std::time::Instant::now();

    loop {
        // Request screenshot
        send_screenshot_request(stream).await?;

        // Receive screenshot response
        match recv_screenshot(stream).await {
            Ok(png_data) => {
                frame_count += 1;

                // Decode PNG → RGBA
                match decode_png_to_rgba(&png_data) {
                    Ok((rgba, width, height)) => {
                        frame_bus.publish(Frame {
                            width,
                            height,
                            rgba,
                            timestamp_us: start.elapsed().as_micros() as u64,
                            h264_nalu: None,
                        });

                        if frame_count % 30 == 1 {
                            let fps = frame_count as f64 / start.elapsed().as_secs_f64();
                            info!(
                                frames = frame_count,
                                fps = format!("{:.1}", fps),
                                "{}x{} capture",
                                width, height
                            );
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "PNG decode failed");
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Screenshot receive failed");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Send DLMessageVersionExchange to screenshotr.
async fn send_version_exchange(stream: &mut tokio::net::TcpStream) -> anyhow::Result<()> {
    // DLMessage format: plist array
    // ["DLMessageVersionExchange", "DLVersionsOk", 300]
    let msg = plist::Value::Array(vec![
        plist::Value::String("DLMessageVersionExchange".to_string()),
        plist::Value::String("DLVersionsOk".to_string()),
        plist::Value::Integer(300.into()),
    ]);

    send_dl_message(stream, &msg).await
}

/// Send screenshot request.
async fn send_screenshot_request(stream: &mut tokio::net::TcpStream) -> anyhow::Result<()> {
    let msg = plist::Value::Array(vec![
        plist::Value::String("DLMessageProcessMessage".to_string()),
        plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("MessageType".to_string(), plist::Value::String("ScreenShotRequest".to_string()));
            d
        }),
    ]);

    send_dl_message(stream, &msg).await
}

/// Receive and parse screenshot response.
async fn recv_screenshot(stream: &mut tokio::net::TcpStream) -> anyhow::Result<Vec<u8>> {
    let data = recv_message(stream).await?;

    // Parse DLMessage response: ["DLMessageProcessMessage", {ScreenShotData: <data>}]
    let cursor = std::io::Cursor::new(&data);
    let value = plist::Value::from_reader(cursor)?;

    if let plist::Value::Array(arr) = value {
        for item in &arr {
            if let plist::Value::Dictionary(dict) = item
                && let Some(plist::Value::Data(png)) = dict.get("ScreenShotData") {
                    return Ok(png.clone());
                }
        }
    }

    Err(anyhow::anyhow!("No ScreenShotData in response"))
}

/// Send a DL message (length-prefixed binary plist).
async fn send_dl_message(stream: &mut tokio::net::TcpStream, value: &plist::Value) -> anyhow::Result<()> {
    let mut body = Vec::new();
    value.to_writer_binary(&mut body)?;

    let len = body.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&body).await?;
    stream.flush().await?;

    Ok(())
}

/// Receive a length-prefixed message.
async fn recv_message(stream: &mut tokio::net::TcpStream) -> anyhow::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len == 0 {
        return Ok(Vec::new());
    }
    if len > 50_000_000 { // 50MB max for a screenshot
        return Err(anyhow::anyhow!("Message too large: {} bytes", len));
    }

    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;

    Ok(data)
}

/// Decode PNG bytes to RGBA pixel buffer.
fn decode_png_to_rgba(png_data: &[u8]) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let cursor = std::io::Cursor::new(png_data);
    let decoder = PngDecoder::new(cursor)?;
    let (width, height) = decoder.dimensions();
    let total_bytes = decoder.total_bytes() as usize;

    let mut rgba = vec![0u8; total_bytes];
    decoder.read_image(&mut rgba)?;

    // If the image is RGB (no alpha), convert to RGBA
    if total_bytes == (width * height * 3) as usize {
        let mut rgba4 = Vec::with_capacity((width * height * 4) as usize);
        for chunk in rgba.chunks(3) {
            rgba4.extend_from_slice(chunk);
            rgba4.push(255);
        }
        return Ok((rgba4, width, height));
    }

    Ok((rgba, width, height))
}
