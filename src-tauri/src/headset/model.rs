use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AmbientMode {
    NoiseCanceling,
    Transparency,
    Normal,
}

impl AmbientMode {
    pub fn as_lib(self) -> &'static str {
        match self {
            AmbientMode::NoiseCanceling => "NoiseCanceling",
            AmbientMode::Transparency => "Transparency",
            AmbientMode::Normal => "Normal",
        }
    }

    pub fn from_lib(s: &str) -> Option<Self> {
        match s {
            "NoiseCanceling" => Some(AmbientMode::NoiseCanceling),
            "Transparency" => Some(AmbientMode::Transparency),
            "Normal" => Some(AmbientMode::Normal),
            _ => None,
        }
    }
}

/// Noise-cancelling strength source. The library calls `Manual` "Custom".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NcMode {
    Adaptive,
    Manual,
}

impl NcMode {
    pub fn as_lib(self) -> &'static str {
        match self {
            NcMode::Adaptive => "Adaptive",
            NcMode::Manual => "Custom",
        }
    }

    pub fn from_lib(s: &str) -> Option<Self> {
        match s {
            "Adaptive" => Some(NcMode::Adaptive),
            "Custom" => Some(NcMode::Manual),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Connection {
    Connecting,
    Connected,
    NotFound,
    Disconnected { retry_in_secs: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub level: u8,
    pub max: u8,
    pub charging: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EqPreset {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Eq {
    pub presets: Vec<EqPreset>,
    /// `None` = custom curve.
    pub preset: Option<String>,
    pub band_hz: Vec<u16>,
    pub values: Vec<i16>,
    pub min: i16,
    pub max: i16,
    pub fraction_digits: i16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadsetState {
    pub connection: Connection,
    pub name: Option<String>,
    pub ambient_mode: Option<AmbientMode>,
    pub nc_mode: Option<NcMode>,
    pub manual_nc: Option<u8>,
    pub adaptive_nc: Option<u8>,
    pub transparency: Option<u8>,
    pub wind_noise: Option<bool>,
    pub battery: Option<Battery>,
    pub eq: Option<Eq>,
    pub firmware: Option<String>,
    pub serial: Option<String>,
}

impl HeadsetState {
    pub fn with_connection(connection: Connection) -> Self {
        Self {
            connection,
            name: None,
            ambient_mode: None,
            nc_mode: None,
            manual_nc: None,
            adaptive_nc: None,
            transparency: None,
            wind_noise: None,
            battery: None,
            eq: None,
            firmware: None,
            serial: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HeadsetCommand {
    SetAmbientMode { mode: AmbientMode },
    SetNcMode { mode: NcMode },
    SetManualNc { level: u8 },
    SetTransparency { level: u8 },
    SetWindNoise { enabled: bool },
    SetEqPreset { preset: String },
    SetEqBands { values: Vec<i16> },
    Retry,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_deserialize_from_tagged_camel_case_json() {
        let c: HeadsetCommand = serde_json::from_str(r#"{"type":"setAmbientMode","mode":"transparency"}"#).unwrap();
        assert_eq!(c, HeadsetCommand::SetAmbientMode { mode: AmbientMode::Transparency });
        let c: HeadsetCommand = serde_json::from_str(r#"{"type":"setNcMode","mode":"manual"}"#).unwrap();
        assert_eq!(c, HeadsetCommand::SetNcMode { mode: NcMode::Manual });
        let c: HeadsetCommand = serde_json::from_str(r#"{"type":"setEqBands","values":[1,2,3]}"#).unwrap();
        assert_eq!(c, HeadsetCommand::SetEqBands { values: vec![1, 2, 3] });
        let c: HeadsetCommand = serde_json::from_str(r#"{"type":"retry"}"#).unwrap();
        assert_eq!(c, HeadsetCommand::Retry);
    }

    #[test]
    fn state_serializes_camel_case_with_tagged_connection() {
        let s = HeadsetState::with_connection(Connection::Disconnected { retry_in_secs: 10 });
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["connection"]["state"], "disconnected");
        assert_eq!(v["connection"]["retryInSecs"], 10);
        assert!(v["ambientMode"].is_null());
        assert!(v["manualNc"].is_null());
    }

    #[test]
    fn library_names_round_trip() {
        for m in [AmbientMode::NoiseCanceling, AmbientMode::Transparency, AmbientMode::Normal] {
            assert_eq!(AmbientMode::from_lib(m.as_lib()), Some(m));
        }
        assert_eq!(NcMode::Manual.as_lib(), "Custom");
        assert_eq!(NcMode::from_lib("Adaptive"), Some(NcMode::Adaptive));
        assert_eq!(NcMode::from_lib("Bogus"), None);
    }
}
