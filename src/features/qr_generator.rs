/// Connection QR code generator: create a QR code with PC's IP and port.
///
/// Display the QR code in the terminal or save as PNG. iPhone can scan it
/// (future: companion app reads QR to auto-configure AirPlay target).

pub fn generate_connection_qr(ip: &str, port: u16) -> String {
    let url = format!("airplay://{}:{}", ip, port);
    // ASCII QR code using simple block characters
    ascii_qr(&url)
}

/// Get the local IP address for display.
pub fn local_ip() -> Result<String, String> {
    let output = std::process::Command::new("powershell")
        .args(["-Command", "(Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.InterfaceAlias -notlike '*Loopback*' -and $_.PrefixOrigin -eq 'Dhcp' }).IPAddress"])
        .output()
        .map_err(|e| format!("Failed to get IP: {}", e))?;

    if output.status.success() {
        let ip = String::from_utf8_lossy(&output.stdout).trim().lines().next().unwrap_or("0.0.0.0").to_string();
        Ok(ip)
    } else {
        Err("Could not determine local IP".to_string())
    }
}

/// Simple ASCII QR-like display (not a real QR encoder, just a formatted box).
/// For a real QR, use the `qrcode` crate.
fn ascii_qr(data: &str) -> String {
    let w = data.len().max(20) + 4;
    let border = "█".repeat(w);
    let empty = format!("██{}██", " ".repeat(w - 4));
    let content = format!("██ {:<width$} ██", data, width = w - 6);

    format!(
        "{border}\n{empty}\n{content}\n{empty}\n{border}\n\nScan or enter: {data}",
        border = border, empty = empty, content = content, data = data
    )
}
