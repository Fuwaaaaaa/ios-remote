use chrono::Local;
use serde::Serialize;
use std::fs;

/// Protocol analyzer: detailed logging of protocol messages (usbmuxd / lockdownd).

#[derive(Debug, Clone, Serialize)]
pub struct ProtocolMessage {
    pub timestamp: String,
    pub direction: Direction,
    pub method: String,
    pub uri: String,
    pub cseq: u32,
    pub content_length: usize,
    pub headers: Vec<(String, String)>,
    pub body_preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum Direction {
    Incoming,
    Outgoing,
}

pub struct ProtocolAnalyzer {
    messages: Vec<ProtocolMessage>,
    log_file: Option<String>,
    max_messages: usize,
}

impl ProtocolAnalyzer {
    pub fn new(log_file: Option<&str>) -> Self {
        Self {
            messages: Vec::new(),
            log_file: log_file.map(|s| s.to_string()),
            max_messages: 10000,
        }
    }

    pub fn log_request(
        &mut self,
        method: &str,
        uri: &str,
        cseq: u32,
        headers: &[(String, String)],
        body: &[u8],
    ) {
        let msg = ProtocolMessage {
            timestamp: Local::now().format("%H:%M:%S%.3f").to_string(),
            direction: Direction::Incoming,
            method: method.to_string(),
            uri: uri.to_string(),
            cseq,
            content_length: body.len(),
            headers: headers.to_vec(),
            body_preview: preview_body(body),
        };
        self.push(msg);
    }

    pub fn log_response(&mut self, cseq: u32, status: u16, body: &[u8]) {
        let msg = ProtocolMessage {
            timestamp: Local::now().format("%H:%M:%S%.3f").to_string(),
            direction: Direction::Outgoing,
            method: format!("{}", status),
            uri: String::new(),
            cseq,
            content_length: body.len(),
            headers: vec![],
            body_preview: preview_body(body),
        };
        self.push(msg);
    }

    fn push(&mut self, msg: ProtocolMessage) {
        if let Some(ref path) = self.log_file
            && let Ok(json) = serde_json::to_string(&msg)
        {
            let _ = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, "{}", json)
                });
        }
        self.messages.push(msg);
        if self.messages.len() > self.max_messages {
            self.messages.remove(0);
        }
    }

    pub fn recent(&self, n: usize) -> &[ProtocolMessage] {
        let start = self.messages.len().saturating_sub(n);
        &self.messages[start..]
    }

    pub fn export_json(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.messages).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }
}

fn preview_body(body: &[u8]) -> String {
    if body.is_empty() {
        return String::new();
    }
    if body.len() <= 64
        && let Ok(s) = std::str::from_utf8(body)
    {
        return s.to_string();
    }
    format!(
        "[{} bytes: {:02X?}...]",
        body.len(),
        &body[..body.len().min(32)]
    )
}
