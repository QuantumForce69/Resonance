//! Settings exactly as openscq30-lib v2.12.0 reports them for a Space One (captured from its demo device).
use std::collections::HashMap;

use openscq30_lib::settings::{Setting, SettingId};

const SETTINGS: &[(SettingId, &str)] = &[
    (SettingId::AmbientSoundMode, r#"{"type":"select","setting":{"options":["NoiseCanceling","Transparency","Normal"],"localizedOptions":["Noise Canceling","Transparency","Normal"]},"value":"NoiseCanceling"}"#),
    (SettingId::NoiseCancelingMode, r#"{"type":"select","setting":{"options":["Custom","Adaptive"],"localizedOptions":["Custom","Adaptive"]},"value":"Custom"}"#),
    (SettingId::AdaptiveNoiseCanceling, r#"{"type":"information","value":"3/5","translatedValue":"3/5"}"#),
    (SettingId::ManualNoiseCanceling, r#"{"type":"i32Range","setting":{"start":1,"end":5,"step":1},"value":4}"#),
    (SettingId::ManualTransparency, r#"{"type":"i32Range","setting":{"start":1,"end":5,"step":1},"value":2}"#),
    (SettingId::WindNoiseSuppression, r#"{"type":"toggle","value":true}"#),
    (SettingId::BatteryLevel, r#"{"type":"information","value":"4/5","translatedValue":"80%"}"#),
    (SettingId::FirmwareVersion, r#"{"type":"information","value":"06.87","translatedValue":"06.87"}"#),
    (SettingId::SerialNumber, r#"{"type":"information","value":"3035705028ABCDEF","translatedValue":"3035705028ABCDEF"}"#),
    (SettingId::PresetEqualizerProfile, r#"{"type":"presetEqualizerProfileSelect","equalizer":{"bandHz":[100,200,400,800,1600,3200,6400,12800],"fractionDigits":1,"min":-60,"max":60},"select":{"options":["SoundcoreSignature","Acoustic","Flat"],"localizedOptions":["Soundcore Signature","Acoustic","Flat"]},"presets":[[0,0,0,0,0,0,0,0],[40,10,20,20,40,40,40,20],[0,0,0,0,0,0,0,0]],"value":"SoundcoreSignature"}"#),
    (SettingId::VolumeAdjustments, r#"{"type":"equalizer","setting":{"bandHz":[100,200,400,800,1600,3200,6400,12800],"fractionDigits":1,"min":-120,"max":134},"readOnly":false,"value":[0,0,0,0,0,0,0,0]}"#),
];

pub fn space_one_settings() -> HashMap<SettingId, Setting> {
    SETTINGS.iter().map(|(id, json)| (*id, serde_json::from_str(json).expect("valid setting json"))).collect()
}
