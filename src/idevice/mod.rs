pub mod device_info;
pub mod file_transfer;
pub mod syslog;

/// Placeholder for idevice crate integration.
///
/// When the `idevice` crate is enabled in Cargo.toml, these modules provide:
/// - Device info (battery, storage, iOS version)
/// - File transfer via AFC protocol
/// - App management via installation_proxy
/// - Syslog relay for real-time device logs
/// - Crash log retrieval
///
/// These features work over USB and are independent of AirPlay.
pub struct _Placeholder;
