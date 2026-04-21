use tracing::info;

/// Text-to-speech: read OCR results aloud via system TTS.
///
/// Uses Windows SAPI (Speech API) via PowerShell.
pub fn speak(text: &str) -> Result<(), String> {
    if text.is_empty() { return Ok(()); }

    // Sanitize text for PowerShell (escape quotes)
    let safe = text.replace('\'', "''").replace('\n', " ");

    let output = std::process::Command::new("powershell")
        .args([
            "-Command",
            &format!("Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.Speak('{}')", safe),
        ])
        .output()
        .map_err(|e| format!("TTS failed: {}", e))?;

    if output.status.success() {
        info!(chars = text.len(), "TTS: spoke text");
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("TTS error: {}", err))
    }
}

/// Speak OCR result from the current frame.
pub fn speak_screen(bus: &super::FrameBus) -> Result<(), String> {
    let frame = bus.latest_frame().ok_or("No frame")?;
    let text = super::ocr::extract_text(&frame, None)?;
    speak(&text)
}
