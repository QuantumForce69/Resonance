use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    engine::{ProcessingSettings, ProcessingView},
    headset::part::HeadsetPart,
    profile::{PartChoice, Profile},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Flow {
    Render,
    Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceState {
    Active,
    Unplugged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Console,
    Communications,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Boost {
    pub db: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

/// A device exactly as the OS reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawDevice {
    pub id: String,
    pub name: String,
    pub flow: Flow,
    pub state: DeviceState,
    pub default_console: bool,
    pub default_comms: bool,
    pub volume: f32,
    pub muted: bool,
    pub boost: Option<Boost>,
}

/// An audio session exactly as the OS reports it. `display_name` is the session's own name or,
/// failing that, the executable's FileDescription.
#[derive(Debug, Clone, PartialEq)]
pub struct RawSession {
    pub id: String,
    pub pid: u32,
    pub exe_path: Option<String>,
    pub display_name: Option<String>,
    pub is_system: bool,
    pub device_id: String,
    pub volume: f32,
    pub muted: bool,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub devices: Vec<RawDevice>,
    pub sessions: Vec<RawSession>,
}

/// Linear peak values (0..1) keyed by session id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Peaks {
    pub sessions: HashMap<String, f32>,
    pub master: f32,
    pub mic: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    /// Alias if set, otherwise the OS name.
    pub name: String,
    pub original_name: String,
    pub alias: Option<String>,
    pub hidden: bool,
    pub flow: Flow,
    pub state: DeviceState,
    pub default_console: bool,
    pub default_comms: bool,
    pub volume: f32,
    pub muted: bool,
    pub boost: Option<Boost>,
}

/// All sessions belonging to one executable, controlled together.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct App {
    pub key: String,
    pub name: String,
    pub exe_path: Option<String>,
    pub is_system: bool,
    pub pids: Vec<u32>,
    pub session_ids: Vec<String>,
    pub volume: f32,
    pub muted: bool,
    pub output: Option<String>,
    /// Send level into calls, if this app is sent.
    pub send: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyMode {
    #[default]
    Toggle,
    PushToTalk,
    PushToMute,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MicConfig {
    pub hotkey: Option<String>,
    pub mode: HotkeyMode,
}

impl Default for MicConfig {
    fn default() -> Self {
        Self { hotkey: Some("Ctrl+Shift+M".into()), mode: HotkeyMode::Toggle }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiSettings {
    pub theme: Theme,
    pub start_minimized: bool,
    /// The first-run setup has been finished or skipped.
    pub onboarded: bool,
    /// Restart audio automatically when a device Resonance was using comes back after disconnecting.
    pub auto_restart_audio: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub devices: Vec<Device>,
    pub apps: Vec<App>,
    pub routing_available: bool,
    pub mic: MicConfig,
    pub ui: UiSettings,
    pub processing: ProcessingView,
    pub profiles: Vec<Profile>,
    pub active_profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterFrame {
    /// Linear peak per app key.
    pub apps: HashMap<String, f32>,
    pub master: f32,
    pub mic: f32,
    /// Processed-mic peaks before and after the chain.
    pub proc_in: f32,
    pub proc_out: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Command {
    SetAppVolume { key: String, volume: f32 },
    SetAppMute { key: String, muted: bool },
    SetAppOutput { key: String, device_id: Option<String> },
    SetDeviceVolume { id: String, volume: f32 },
    SetDeviceMute { id: String, muted: bool },
    SetDefaultDevice { id: String, role: Role },
    SetDeviceAlias { id: String, alias: Option<String> },
    SetDeviceHidden { id: String, hidden: bool },
    SetMicBoost { id: String, db: f32 },
    SetMicMute { muted: bool },
    ToggleMicMute,
    SetMicConfig { config: MicConfig },
    SetUiSettings { ui: UiSettings },
    ResetAppMemory,
    SetProcessing { processing: ProcessingSettings },
    SetAppSend { key: String, level: Option<f32> },
    SaveProfile { name: String, parts: PartChoice, headset: Option<HeadsetPart> },
    UpdateProfile { id: String, headset: Option<HeadsetPart> },
    RenameProfile { id: String, name: String },
    DeleteProfile { id: String },
    ApplyProfile { id: String },
    /// Rebuild the backend and engine (Restart audio). Handled by the worker itself, never by `AppCore::apply`.
    RestartAudio,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_deserializes_from_tagged_camel_case_json() {
        let c: Command =
            serde_json::from_str(r#"{"type":"setAppVolume","key":"spotify.exe","volume":0.5}"#).unwrap();
        assert_eq!(c, Command::SetAppVolume { key: "spotify.exe".into(), volume: 0.5 });

        let c: Command =
            serde_json::from_str(r#"{"type":"setAppOutput","key":"a.exe","deviceId":null}"#).unwrap();
        assert_eq!(c, Command::SetAppOutput { key: "a.exe".into(), device_id: None });

        let c: Command =
            serde_json::from_str(r#"{"type":"setDefaultDevice","id":"x","role":"communications"}"#).unwrap();
        assert_eq!(c, Command::SetDefaultDevice { id: "x".into(), role: Role::Communications });

        let c: Command = serde_json::from_str(r#"{"type":"toggleMicMute"}"#).unwrap();
        assert_eq!(c, Command::ToggleMicMute);

        let c: Command = serde_json::from_str(
            r#"{"type":"setMicConfig","config":{"hotkey":"Ctrl+F13","mode":"pushToTalk"}}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::SetMicConfig {
                config: MicConfig { hotkey: Some("Ctrl+F13".into()), mode: HotkeyMode::PushToTalk }
            }
        );

        let c: Command =
            serde_json::from_str(r#"{"type":"setProcessing","processing":{"enabled":true,"monitorLevel":0.5}}"#).unwrap();
        let Command::SetProcessing { processing } = c else { panic!("wrong command") };
        assert!(processing.enabled);
        assert_eq!(processing.monitor_level, 0.5);

        let c: Command = serde_json::from_str(r#"{"type":"setAppSend","key":"spotify.exe","level":null}"#).unwrap();
        assert_eq!(c, Command::SetAppSend { key: "spotify.exe".into(), level: None });

        let c: Command = serde_json::from_str(
            r#"{"type":"saveProfile","name":"Gaming","parts":{"apps":true,"defaultOutput":true},"headset":null}"#,
        )
        .unwrap();
        assert_eq!(
            c,
            Command::SaveProfile {
                name: "Gaming".into(),
                parts: PartChoice { apps: true, default_output: true, ..Default::default() },
                headset: None
            }
        );
    }

    #[test]
    fn ui_state_serializes_camel_case() {
        let s = UiState { routing_available: true, ..Default::default() };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["routingAvailable"], true);
        assert_eq!(v["ui"]["startMinimized"], false);
        assert_eq!(v["mic"]["hotkey"], "Ctrl+Shift+M");
        assert_eq!(v["mic"]["mode"], "toggle");
        assert_eq!(v["processing"]["status"]["state"], "off");
        assert_eq!(v["ui"]["onboarded"], false);
        assert!(v["activeProfile"].is_null());
    }

    #[test]
    fn audio_error_serializes_code_and_message() {
        let v = serde_json::to_value(crate::error::AudioError::NotFound("gone".into())).unwrap();
        assert_eq!(v["code"], "notFound");
        assert_eq!(v["message"], "gone");
    }
}
