use tracing::info;

/// Sound notifications: play a WAV file when events occur.
pub fn play_sound(wav_path: &str) -> Result<(), String> {
    // Use PowerShell to play WAV (no extra deps needed)
    let output = std::process::Command::new("powershell")
        .args([
            "-Command",
            &format!(
                "(New-Object Media.SoundPlayer '{}').PlaySync()",
                wav_path.replace('\'', "''")
            ),
        ])
        .output()
        .map_err(|e| format!("Sound play failed: {}", e))?;

    if output.status.success() {
        info!(file = %wav_path, "Sound played");
        Ok(())
    } else {
        Err("Sound playback failed".to_string())
    }
}

/// Play the Windows system notification sound.
pub fn play_system_notification() -> Result<(), String> {
    let _output = std::process::Command::new("powershell")
        .args(["-Command", "[System.Media.SystemSounds]::Asterisk.Play()"])
        .output()
        .map_err(|e| format!("System sound failed: {}", e))?;
    Ok(())
}
