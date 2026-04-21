use tracing::info;

/// Drag & drop file transfer: drop files onto the window to send to iPhone.
///
/// Listens for file drop events on the display window and triggers
/// AFC file transfer to the iPhone.
/// Handle a file drop event from the display window.
pub fn handle_file_drop(paths: &[String]) {
    for path in paths {
        info!(file = %path, "File dropped — queued for transfer");
        // TODO: integrate with idevice::file_transfer::upload_file
    }
}

/// Handle a drag-out event (copy file from iPhone to PC).
/// Returns the local path where the file was saved.
pub fn handle_drag_out(remote_path: &str, local_dir: &str) -> Result<String, String> {
    let filename = std::path::Path::new(remote_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let local_path = format!("{}/{}", local_dir, filename);
    info!(from = %remote_path, to = %local_path, "Drag out — file transfer");
    // TODO: integrate with idevice::file_transfer::download_file
    Ok(local_path)
}
