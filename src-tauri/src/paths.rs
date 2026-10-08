use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// Moves an unreadable file aside as `<stem>.bad-<unix seconds>.<ext>` and returns the new path.
pub fn quarantine(path: &Path) -> PathBuf {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let backup = path.with_file_name(format!("{stem}.bad-{ts}{ext}"));
    let _ = std::fs::rename(path, &backup);
    backup
}

/// `%APPDATA%\Resonance` (falls back to the current directory if APPDATA is unset).
pub fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("Resonance")
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}
