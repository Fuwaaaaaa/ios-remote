use serde::Serialize;
use std::path::PathBuf;

/// File entry from iPhone filesystem (via AFC protocol).
#[derive(Debug, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// List files in an AFC-accessible directory.
///
/// AFC gives access to app documents, media folders, and crash logs.
/// General filesystem access is not available without jailbreak.
pub async fn list_files(_path: &str) -> Result<Vec<FileEntry>, String> {
    Err("idevice AFC not yet enabled".to_string())
}

/// Download a file from iPhone to local path.
pub async fn download_file(_remote_path: &str, _local_path: &PathBuf) -> Result<(), String> {
    Err("idevice AFC not yet enabled".to_string())
}

/// Upload a file from local path to iPhone.
pub async fn upload_file(_local_path: &PathBuf, _remote_path: &str) -> Result<(), String> {
    Err("idevice AFC not yet enabled".to_string())
}
