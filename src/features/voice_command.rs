use tracing::info;

/// Voice commands: hands-free control via speech recognition.
///
/// Uses Windows Speech Recognition or Whisper API to listen for commands.
/// "screenshot", "record", "stop", "zoom in", "quit"
pub struct VoiceCommands {
    pub enabled: bool,
    pub commands: Vec<VoiceMapping>,
}

#[derive(Clone)]
pub struct VoiceMapping {
    pub phrase: String,
    pub action: String, // command palette ID
}

impl VoiceCommands {
    pub fn new() -> Self {
        Self {
            enabled: false,
            commands: vec![
                VoiceMapping { phrase: "screenshot".into(), action: "screenshot".into() },
                VoiceMapping { phrase: "スクリーンショット".into(), action: "screenshot".into() },
                VoiceMapping { phrase: "record".into(), action: "record_start".into() },
                VoiceMapping { phrase: "録画".into(), action: "record_start".into() },
                VoiceMapping { phrase: "stop".into(), action: "record_stop".into() },
                VoiceMapping { phrase: "停止".into(), action: "record_stop".into() },
                VoiceMapping { phrase: "zoom in".into(), action: "zoom_in".into() },
                VoiceMapping { phrase: "quit".into(), action: "quit".into() },
                VoiceMapping { phrase: "終了".into(), action: "quit".into() },
                VoiceMapping { phrase: "ocr".into(), action: "ocr".into() },
            ],
        }
    }

    /// Match recognized speech to a command.
    pub fn match_speech(&self, text: &str) -> Option<&str> {
        let lower = text.to_lowercase();
        for cmd in &self.commands {
            if lower.contains(&cmd.phrase.to_lowercase()) {
                info!(phrase = %cmd.phrase, action = %cmd.action, "Voice command matched");
                return Some(&cmd.action);
            }
        }
        None
    }

    /// Start listening via Windows Speech API (blocking, run on thread).
    pub fn listen_windows(&self) -> Result<String, String> {
        let output = std::process::Command::new("powershell")
            .args(["-Command", r#"
Add-Type -AssemblyName System.Speech
$r = New-Object System.Speech.Recognition.SpeechRecognitionEngine
$r.SetInputToDefaultAudioDevice()
$g = New-Object System.Speech.Recognition.DictationGrammar
$r.LoadGrammar($g)
$result = $r.Recognize()
$result.Text
"#])
            .output()
            .map_err(|e| format!("Speech recognition failed: {}", e))?;

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() { Err("No speech detected".into()) } else { Ok(text) }
    }
}
