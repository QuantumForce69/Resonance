use std::path::PathBuf;

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
