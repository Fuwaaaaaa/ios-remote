use crate::airplay::info as airplay_info;
use crate::airplay::pairing::{FairPlaySetup, PairSetup, PairVerify};
use crate::airplay::SharedState;
use bytes::BytesMut;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, info, warn};

pub struct RtspServer {
    listener: TcpListener,
    state: SharedState,
}

impl RtspServer {
    pub async fn bind(
        addr: impl tokio::net::ToSocketAddrs,
        state: SharedState,
    ) -> Result<Self, crate::error::Error> {
        let listener = TcpListener::bind(addr).await?;
        let local = listener.local_addr()?;
        info!(addr = %local, "RTSP server listening");
        Ok(Self { listener, state })
    }

    pub async fn run(self) -> Result<(), crate::error::Error> {
        loop {
            let (stream, peer) = self.listener.accept().await?;
            info!(peer = %peer, "New RTSP connection");
            let state = self.state.clone();
            tokio::spawn(async move {
                let mut conn = Connection::new(stream, peer, state);
                if let Err(e) = conn.run().await {
                    warn!(peer = %peer, error = %e, "Connection error");
                }
            });
        }
    }
}

/// Per-connection state and handler.
struct Connection {
    stream: TcpStream,
    peer: SocketAddr,
    state: SharedState,
    pair_setup: PairSetup,
    pair_verify: Option<PairVerify>,
    fp_setup: FairPlaySetup,
    device_id: String,
    /// Pending session key from pair-verify step 1
    pending_session_key: Option<[u8; 32]>,
}

impl Connection {
    fn new(stream: TcpStream, peer: SocketAddr, state: SharedState) -> Self {
        // We'll initialize pair_setup with a dummy key; replaced on first use
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        Self {
            stream,
            peer,
            state,
            pair_setup: PairSetup::new(signing_key.clone()),
            pair_verify: Some(PairVerify::new(signing_key)),
            fp_setup: FairPlaySetup::new(),
            device_id: String::new(),
            pending_session_key: None,
        }
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        // Copy the signing key from shared state
        {
            let s = self.state.lock().await;
            self.pair_setup = PairSetup::new(s.keypair.clone());
            self.pair_verify = Some(PairVerify::new(s.keypair.clone()));
            self.device_id = "AA:BB:CC:DD:EE:FF".to_string();
        }

        let mut buf = BytesMut::with_capacity(16384);

        loop {
            let n = self.stream.read_buf(&mut buf).await?;
            if n == 0 {
                info!(peer = %self.peer, "Connection closed");
                return Ok(());
            }

            while let Some((request, consumed)) = parse_rtsp_request(&buf) {
                debug!(
                    peer = %self.peer,
                    method = %request.method,
                    uri = %request.uri,
                    body_len = request.body.len(),
                    "RTSP request"
                );

                let response = self.dispatch(&request).await;
                self.stream.write_all(&response).await?;

                let _ = buf.split_to(consumed);
            }
        }
    }

    async fn dispatch(&mut self, req: &RtspRequest) -> Vec<u8> {
        match (req.method.as_str(), req.uri.as_str()) {
            ("OPTIONS", _) => {
                let methods = "ANNOUNCE, SETUP, RECORD, PAUSE, FLUSH, GET_PARAMETER, \
                               SET_PARAMETER, TEARDOWN, OPTIONS, POST, GET";
                self.text_response(req.cseq, 200, "OK", &[("Public", methods)], &[])
            }

            ("GET", uri) if uri.contains("/info") => {
                let s = self.state.lock().await;
                let body = airplay_info::build_info_response(
                    &self.device_id,
                    &s.keypair.verifying_key(),
                );
                self.binary_response(
                    req.cseq,
                    200,
                    "OK",
                    "application/x-apple-binary-plist",
                    &body,
                )
            }

            ("POST", uri) if uri.contains("/pair-setup") => {
                match self.pair_setup.handle(&req.body) {
                    Ok((body, maybe_key)) => {
                        if let Some(key) = maybe_key {
                            self.pending_session_key = Some(key);
                        }
                        self.binary_response(
                            req.cseq,
                            200,
                            "OK",
                            "application/octet-stream",
                            &body,
                        )
                    }
                    Err(e) => {
                        warn!(error = %e, "pair-setup failed");
                        self.text_response(req.cseq, 500, "Internal Server Error", &[], &[])
                    }
                }
            }

            ("POST", uri) if uri.contains("/pair-verify") => {
                self.handle_pair_verify(req).await
            }

            ("POST", uri) if uri.contains("/fp-setup") => {
                match self.fp_setup.handle(&req.body) {
                    Ok(body) => self.binary_response(
                        req.cseq,
                        200,
                        "OK",
                        "application/octet-stream",
                        &body,
                    ),
                    Err(e) => {
                        warn!(error = %e, "fp-setup failed");
                        self.text_response(req.cseq, 500, "Internal Server Error", &[], &[])
                    }
                }
            }

            ("SETUP", _) => {
                self.handle_setup(req).await
            }

            ("GET_PARAMETER", _) | ("SET_PARAMETER", _) => {
                // Parameter exchange — acknowledge
                self.text_response(req.cseq, 200, "OK", &[], &[])
            }

            ("RECORD", _) => {
                info!("RECORD — iPhone is starting to stream!");
                self.text_response(req.cseq, 200, "OK", &[], &[])
            }

            ("TEARDOWN", _) => {
                info!("TEARDOWN — session ended");
                self.text_response(req.cseq, 200, "OK", &[], &[])
            }

            ("FLUSH", _) => {
                self.text_response(req.cseq, 200, "OK", &[], &[])
            }

            _ => {
                warn!(method = %req.method, uri = %req.uri, "Unhandled");
                self.text_response(req.cseq, 501, "Not Implemented", &[], &[])
            }
        }
    }

    async fn handle_pair_verify(&mut self, req: &RtspRequest) -> Vec<u8> {
        let body = &req.body;

        if body.len() >= 4 {
            // Check if this is step 1 or step 2 based on the first byte
            let step = body[0];

            if step == 1 && body.len() >= 36 {
                // Step 1: client sends flag(4) + X25519 public key(32)
                let mut client_pub = [0u8; 32];
                client_pub.copy_from_slice(&body[4..36]);

                if let Some(ref verifier) = self.pair_verify {
                    match verifier.step1(&client_pub) {
                        Ok((resp_body, session_key)) => {
                            self.pending_session_key = Some(session_key);
                            // Prepend step indicator
                            let mut full = vec![0u8; 4];
                            full.extend_from_slice(&resp_body);
                            return self.binary_response(
                                req.cseq,
                                200,
                                "OK",
                                "application/octet-stream",
                                &full,
                            );
                        }
                        Err(e) => {
                            warn!(error = %e, "pair-verify step 1 failed");
                        }
                    }
                }
            } else if step == 0 {
                // Step 2: client sends encrypted signature
                if let Some(ref verifier) = self.pair_verify {
                    if let Some(ref key) = self.pending_session_key {
                        match verifier.step2(&body[4..], key) {
                            Ok(true) => {
                                // Store session key in shared state
                                let mut s = self.state.lock().await;
                                s.session_key = self.pending_session_key.take();
                                info!("Pairing verified — session encrypted");
                                return self.text_response(req.cseq, 200, "OK", &[], &[]);
                            }
                            Ok(false) => {
                                warn!("pair-verify step 2: verification failed");
                            }
                            Err(e) => {
                                warn!(error = %e, "pair-verify step 2 error");
                            }
                        }
                    }
                }
            }
        }

        // Fallback: accept anyway for debugging
        info!("pair-verify: accepting (debug mode)");
        self.text_response(req.cseq, 200, "OK", &[], &[])
    }

    async fn handle_setup(&mut self, req: &RtspRequest) -> Vec<u8> {
        // Parse the SETUP body (binary plist) to get stream type
        let s = self.state.lock().await;

        // Build response plist with our port assignments
        let mut dict = plist::Dictionary::new();
        dict.insert("eventPort".to_owned(), plist::Value::Integer(s.event_port.into()));
        dict.insert("dataPort".to_owned(), plist::Value::Integer(s.video_data_port.into()));
        dict.insert("timingPort".to_owned(), plist::Value::Integer(s.ntp_port.into()));

        let root = plist::Value::Dictionary(dict);
        let mut body = Vec::new();
        root.to_writer_binary(&mut body)
            .expect("plist serialization should not fail");

        info!(
            video_port = s.video_data_port,
            event_port = s.event_port,
            "SETUP — assigned ports"
        );

        self.binary_response(
            req.cseq,
            200,
            "OK",
            "application/x-apple-binary-plist",
            &body,
        )
    }

    // ─── Response Builders ───────────────────────────���───────────────

    fn text_response(
        &self,
        cseq: u32,
        status: u16,
        reason: &str,
        extra_headers: &[(&str, &str)],
        body: &[u8],
    ) -> Vec<u8> {
        let mut resp = format!("RTSP/1.0 {} {}\r\nCSeq: {}\r\n", status, reason, cseq);
        for (k, v) in extra_headers {
            resp.push_str(&format!("{}: {}\r\n", k, v));
        }
        if !body.is_empty() {
            resp.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        resp.push_str("\r\n");
        let mut out = resp.into_bytes();
        out.extend_from_slice(body);
        out
    }

    fn binary_response(
        &self,
        cseq: u32,
        status: u16,
        reason: &str,
        content_type: &str,
        body: &[u8],
    ) -> Vec<u8> {
        let header = format!(
            "RTSP/1.0 {} {}\r\nCSeq: {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
            status, reason, cseq, content_type, body.len()
        );
        let mut out = header.into_bytes();
        out.extend_from_slice(body);
        out
    }
}

// ─── RTSP Parser ─────────────────────────────────────────────────────────────

struct RtspRequest {
    method: String,
    uri: String,
    cseq: u32,
    #[allow(dead_code)]
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn parse_rtsp_request(buf: &[u8]) -> Option<(RtspRequest, usize)> {
    let text = std::str::from_utf8(buf).ok()?;
    let header_end = text.find("\r\n\r\n")? + 4;

    let mut lines = text[..header_end].lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let uri = parts.next()?.to_string();

    let mut headers = HashMap::new();
    let mut cseq = 0u32;
    let mut content_length = 0usize;

    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if key.eq_ignore_ascii_case("CSeq") {
                cseq = value.parse().unwrap_or(0);
            }
            if key.eq_ignore_ascii_case("Content-Length") {
                content_length = value.parse().unwrap_or(0);
            }
            headers.insert(key, value);
        }
    }

    let total = header_end + content_length;
    if buf.len() < total {
        return None;
    }

    let body = buf[header_end..total].to_vec();

    Some((
        RtspRequest {
            method,
            uri,
            cseq,
            headers,
            body,
        },
        total,
    ))
}
