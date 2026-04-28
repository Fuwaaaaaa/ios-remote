//! `--diag` one-shot diagnostic dump.
//!
//! Walks the usbmuxd → lockdownd → screenshotr chain and prints every
//! intermediate response (raw XML plist) to stdout. Used when a user reports a
//! "Trust" or "screen does not display" issue on a device this build does not
//! fully support (notably iOS 17+, which requires Pairing/StartSession/TLS and
//! a Personalized DDI mounted via RemoteXPC — none of which this build does).
//!
//! Output is intentionally human-readable plain text (with embedded XML
//! plist excerpts) so users can paste it into an issue without further
//! processing.

use super::lockdown::{self, LockdownClient};
use super::usbmuxd::{UsbDevice, UsbmuxdClient};
use plist::{Dictionary, Value};

/// Run the diagnostic and print results to stdout. Always returns `Ok(())`
/// (failures are reported in-band as text); callers can `std::process::exit`
/// after this returns.
pub async fn run() -> anyhow::Result<()> {
    println!("=== ios-remote --diag ===");
    println!("crate version : {}", env!("CARGO_PKG_VERSION"));
    println!();

    println!("--- usbmuxd ---");
    let mut mux = match UsbmuxdClient::connect().await {
        Ok(c) => c,
        Err(e) => {
            println!("usbmuxd connect FAILED: {e}");
            println!(
                "Hint: install iTunes or 'Apple Devices' from the Microsoft Store \
                 to get the Apple Mobile Device Service running on 127.0.0.1:27015."
            );
            return Ok(());
        }
    };
    println!("usbmuxd connect : OK (127.0.0.1:27015)");

    let devices = match mux.list_devices().await {
        Ok(d) => d,
        Err(e) => {
            println!("ListDevices FAILED: {e}");
            return Ok(());
        }
    };

    if devices.is_empty() {
        println!("No iPhone connected.");
        return Ok(());
    }
    println!("Devices: {}", devices.len());
    for d in &devices {
        println!(
            "  - device_id={} udid={} conn={}",
            d.device_id, d.udid, d.connection_type
        );
    }
    println!();

    for d in &devices {
        diag_one_device(d).await;
    }

    println!("=== end of diag ===");
    Ok(())
}

async fn diag_one_device(device: &UsbDevice) {
    println!("--- device {} ({}) ---", device.udid, device.connection_type);

    let mut mux = match UsbmuxdClient::connect().await {
        Ok(c) => c,
        Err(e) => {
            println!("  usbmuxd reconnect FAILED: {e}");
            return;
        }
    };

    let mut lockdown = match LockdownClient::connect(&mut mux, device.device_id).await {
        Ok(c) => c,
        Err(e) => {
            println!("  lockdownd connect FAILED: {e}");
            return;
        }
    };
    println!("  lockdownd connect : OK (port 62078 over USB tunnel)");

    let keys = [
        ("ProductVersion", None),
        ("ProductType", None),
        ("BuildVersion", None),
        ("DeviceName", None),
        ("DeviceClass", None),
        ("UniqueDeviceID", None),
        ("CPUArchitecture", None),
        ("HardwareModel", None),
        ("HostAttached", Some("com.apple.mobile.lockdown")),
    ];
    let mut ios_version_str = String::new();
    for (key, domain) in keys {
        match lockdown.get_value(domain, key).await {
            Ok(v) => {
                let pretty = render_value(&v);
                println!(
                    "  GetValue {:<22} {:<32} = {}",
                    domain.unwrap_or("<no-domain>"),
                    key,
                    pretty
                );
                if key == "ProductVersion"
                    && let Some(s) = v.as_string()
                {
                    ios_version_str = s.to_string();
                }
            }
            Err(e) => {
                println!(
                    "  GetValue {:<22} {:<32} FAILED: {}",
                    domain.unwrap_or("<no-domain>"),
                    key,
                    e
                );
            }
        }
    }

    if let Some(major) = lockdown::parse_ios_major(&ios_version_str)
        && major >= 17
    {
        println!();
        println!(
            "  NOTE: iOS {major}+ detected ({ios_version_str}). screenshotr will almost \
             certainly fail because this build does not implement: ReadPairRecord / \
             StartSession / TLS upgrade / Personalized DDI mount via RemoteXPC."
        );
    }

    println!();
    println!("  Attempting StartService 'com.apple.mobile.screenshotr'...");

    let mut req = Dictionary::new();
    req.insert(
        "Request".to_string(),
        Value::String("StartService".to_string()),
    );
    req.insert(
        "Service".to_string(),
        Value::String("com.apple.mobile.screenshotr".to_string()),
    );
    match lockdown.raw_request(req).await {
        Ok(resp) => {
            println!("  raw response (XML plist):");
            for line in LockdownClient::dict_to_xml(&resp).lines() {
                println!("    {line}");
            }
        }
        Err(e) => println!("  StartService transport error: {e}"),
    }

    println!();
}

fn render_value(v: &Value) -> String {
    match v {
        Value::String(s) => format!("\"{s}\""),
        Value::Boolean(b) => format!("{b}"),
        Value::Integer(i) => format!("{i}"),
        Value::Real(r) => format!("{r}"),
        Value::Data(d) => format!("<{} bytes of binary data>", d.len()),
        Value::Date(_) => "<date>".to_string(),
        Value::Uid(_) => "<uid>".to_string(),
        Value::Array(a) => format!("<array len={}>", a.len()),
        Value::Dictionary(d) => format!("<dict keys={}>", d.len()),
        _ => "<unknown>".to_string(),
    }
}
