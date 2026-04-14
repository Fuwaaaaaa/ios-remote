#[derive(Debug, thiserror::Error)]
pub enum AirPlayError {
    #[error("mDNS registration failed: {0}")]
    MdnsRegistration(String),

    #[error("RTSP error: {0}")]
    Rtsp(String),

    #[error("pairing failed: {0}")]
    Pairing(String),

    #[error("FairPlay setup failed: {0}")]
    FairPlay(String),

    #[error("stream error: {0}")]
    Stream(String),

    #[error("connection closed by peer")]
    ConnectionClosed,

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
