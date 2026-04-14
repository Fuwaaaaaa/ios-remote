use std::path::PathBuf;
use tracing::info;

/// Portable mode: store all config/data next to the executable.
///
/// When `portable.marker` file exists next to the exe, all paths
/// (config, recordings, screenshots, etc.) are relative to exe dir
/// instead of the current working directory.

pub fn is_portable() -> bool {
    exe_dir().join("portable.marker").exists()
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("."))
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf()
}

/// Get the data directory (portable = exe dir, normal = cwd).
pub fn data_dir() -> PathBuf {
    if is_portable() {
        let dir = exe_dir().join("data");
        let _ = std::fs::create_dir_all(&dir);
        dir
    } else {
        PathBuf::from(".")
    }
}

/// Enable portable mode by creating the marker file.
pub fn enable_portable() -> Result<(), String> {
    let marker = exe_dir().join("portable.marker");
    std::fs::write(&marker, "ios-remote portable mode").map_err(|e| e.to_string())?;
    let _ = std::fs::create_dir_all(exe_dir().join("data"));
    info!(path = %marker.display(), "Portable mode enabled");
    Ok(())
}
