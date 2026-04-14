#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("mDNS: {0}")]
    Mdns(String),

    #[error("RTSP: {0}")]
    Rtsp(String),

    #[error("pairing: {0}")]
    Pairing(String),

    #[error("FairPlay: {0}")]
    FairPlay(String),

    #[error("stream: {0}")]
    Stream(String),

    #[error("recording: {0}")]
    Recording(String),

    #[error("idevice: {0}")]
    IDevice(String),

    #[error("display: {0}")]
    Display(String),

    #[error("connection closed")]
    ConnectionClosed,

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
