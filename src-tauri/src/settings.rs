use std::{collections::BTreeMap, fs, io, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    engine::{ProcessingSettings, RestoreDefaults},
    model::{MicConfig, UiSettings},
};

pub const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppPrefs {
    pub volume: f32,
    pub muted: bool,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self { volume: 1.0, muted: false }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DevicePrefs {
    pub alias: Option<String>,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    pub apps: BTreeMap<String, AppPrefs>,
    pub devices: BTreeMap<String, DevicePrefs>,
    pub mic: MicConfig,
    pub ui: UiSettings,
    pub processing: ProcessingSettings,
    /// Default mics to restore; present while processing has them pointed at VB-Cable.
    pub processing_restore: Option<RestoreDefaults>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            apps: BTreeMap::new(),
            devices: BTreeMap::new(),
            mic: MicConfig::default(),
            ui: UiSettings::default(),
            processing: ProcessingSettings::default(),
            processing_restore: None,
        }
    }
}

impl Settings {
    /// Loads settings; never fails. A corrupt file is renamed to `settings.bad-<unix>.json`.
    pub fn load(path: &Path) -> Settings {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Settings::default(),
            Err(e) => {
                tracing::warn!("couldn't read settings {}: {e}", path.display());
                return Settings::default();
            }
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(s) => s,
            Err(e) => {
                let backup = crate::paths::quarantine(path);
                tracing::warn!("settings file is corrupt ({e}); moved to {}", backup.display());
                Settings::default()
            }
        }
    }

    /// Writes atomically (temp file + rename), creating the directory if needed.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(io::Error::other)?)?;
        fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HotkeyMode, Theme};

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::load(&dir.path().join("settings.json"));
        assert_eq!(s, Settings::default());
        assert_eq!(s.version, SETTINGS_VERSION);
        assert_eq!(s.mic.hotkey.as_deref(), Some("Ctrl+Shift+M"));
    }

    #[test]
    fn round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("settings.json");
        let mut s = Settings::default();
        s.apps.insert("spotify.exe".into(), AppPrefs { volume: 0.4, muted: true });
        s.devices.insert("dev".into(), DevicePrefs { alias: Some("Desk".into()), hidden: true });
        s.mic.mode = HotkeyMode::PushToMute;
        s.ui.theme = Theme::Light;
        s.processing.enabled = true;
        s.processing.dsp.gate.threshold_db = -50.0;
        s.processing_restore = Some(RestoreDefaults { console: Some("mic1".into()), comms: None });
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"startMinimized\""));
        assert!(text.contains("\"processingRestore\""));
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaults_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        let s = Settings::load(&path);
        assert_eq!(s, Settings::default());
        assert!(!path.exists());
        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("settings.bad-"))
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"version":1,"apps":{"a.exe":{"volume":0.2}},"future":true}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.apps["a.exe"], AppPrefs { volume: 0.2, muted: false });
        assert_eq!(s.mic, crate::model::MicConfig::default());
    }
}
