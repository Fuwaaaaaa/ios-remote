use tracing::info;

/// Battery saver: reduce processing when laptop is on battery power.

pub struct BatterySaver {
    pub enabled: bool,
    pub on_battery: bool,
    pub fps_cap: u32,
    pub disable_overlays: bool,
    pub disable_recording: bool,
}

impl BatterySaver {
    pub fn new() -> Self {
        Self {
            enabled: true,
            on_battery: false,
            fps_cap: 15,
            disable_overlays: true,
            disable_recording: true,
        }
    }

    /// Check if running on battery (Windows).
    pub fn check_power_status(&mut self) {
        let output = std::process::Command::new("powershell")
            .args(["-Command", "(Get-WmiObject Win32_Battery).BatteryStatus"])
            .output();

        self.on_battery = match output {
            Ok(o) => {
                let status = String::from_utf8_lossy(&o.stdout).trim().to_string();
                // BatteryStatus: 1=discharging, 2=AC power
                status == "1"
            }
            Err(_) => false, // No battery (desktop PC)
        };
    }

    pub fn should_limit(&self) -> bool { self.enabled && self.on_battery }
    pub fn target_fps(&self) -> u32 { if self.should_limit() { self.fps_cap } else { 60 } }
}

/// Web dashboard API token authentication.
pub struct ApiAuth {
    tokens: Vec<String>,
    enabled: bool,
}

impl ApiAuth {
    pub fn new() -> Self {
        let token = std::env::var("IOS_REMOTE_API_TOKEN").ok();
        Self {
            tokens: token.into_iter().collect(),
            enabled: false,
        }
    }

    pub fn enable(&mut self, token: &str) {
        self.tokens.push(token.to_string());
        self.enabled = true;
        info!("API authentication enabled");
    }

    pub fn validate(&self, header: &str) -> bool {
        if !self.enabled { return true; }
        let token = header.strip_prefix("Bearer ").unwrap_or(header);
        self.tokens.iter().any(|t| t == token)
    }
}
