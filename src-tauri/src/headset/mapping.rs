use std::borrow::Cow;

use openscq30_lib::settings::{Setting, SettingId, Value};

use super::model::*;
use crate::error::{AudioError, Result};

/// Anything that can answer "what is this setting right now" — the real device or a test map.
pub trait SettingSource {
    fn setting(&self, id: &SettingId) -> Option<Setting>;
}

fn select(src: &dyn SettingSource, id: SettingId) -> Option<String> {
    match src.setting(&id)? {
        Setting::Select { value, .. } => Some(value.into_owned()),
        _ => None,
    }
}

fn information(src: &dyn SettingSource, id: SettingId) -> Option<String> {
    match src.setting(&id)? {
        Setting::Information { value, .. } => Some(value),
        _ => None,
    }
}

fn level(src: &dyn SettingSource, id: SettingId) -> Option<u8> {
    match src.setting(&id)? {
        Setting::I32Range { value, .. } => u8::try_from(value).ok(),
        _ => None,
    }
}

fn toggle(src: &dyn SettingSource, id: SettingId) -> Option<bool> {
    match src.setting(&id)? {
        Setting::Toggle { value } => Some(value),
        _ => None,
    }
}

/// Parses the library's `"<n>/<max>"` information strings.
fn fraction(text: &str) -> Option<(u8, u8)> {
    let (a, b) = text.split_once('/')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

fn read_eq(src: &dyn SettingSource) -> Option<Eq> {
    let Setting::Equalizer { setting, value, .. } = src.setting(&SettingId::VolumeAdjustments)? else {
        return None;
    };
    let (presets, preset) = match src.setting(&SettingId::PresetEqualizerProfile) {
        Some(Setting::PresetEqualizerProfileSelect { select, value, .. }) => (
            select
                .options
                .iter()
                .zip(select.localized_options.iter())
                .map(|(id, name)| EqPreset { id: id.to_string(), name: name.clone() })
                .collect(),
            value.map(Cow::into_owned),
        ),
        _ => (Vec::new(), None),
    };
    Some(Eq {
        presets,
        preset,
        band_hz: setting.band_hz.to_vec(),
        values: value,
        min: setting.min,
        max: setting.max,
        fraction_digits: setting.fraction_digits,
    })
}

/// Builds the UI state for a connected headset from its current settings.
pub fn read_state(src: &dyn SettingSource, name: &str) -> HeadsetState {
    let mut s = HeadsetState::with_connection(Connection::Connected);
    s.name = Some(name.to_string());
    s.ambient_mode = select(src, SettingId::AmbientSoundMode).and_then(|v| AmbientMode::from_lib(&v));
    s.nc_mode = select(src, SettingId::NoiseCancelingMode).and_then(|v| NcMode::from_lib(&v));
    s.adaptive_nc = information(src, SettingId::AdaptiveNoiseCanceling).and_then(|v| fraction(&v)).map(|(n, _)| n);
    s.manual_nc = level(src, SettingId::ManualNoiseCanceling);
    s.transparency = level(src, SettingId::ManualTransparency);
    s.wind_noise = toggle(src, SettingId::WindNoiseSuppression);
    s.battery = information(src, SettingId::BatteryLevel).and_then(|v| fraction(&v)).map(|(level, max)| Battery {
        level,
        max,
        charging: information(src, SettingId::IsCharging).map(|v| v == "true"),
    });
    s.firmware = information(src, SettingId::FirmwareVersion);
    s.serial = information(src, SettingId::SerialNumber);
    s.eq = read_eq(src);
    s
}

fn eq_of(state: &HeadsetState) -> Result<&Eq> {
    state.eq.as_ref().ok_or_else(|| AudioError::Unavailable("The headphones' EQ isn't available".into()))
}

/// Translates a UI command into library setting writes, validating and clamping values.
pub fn command_values(cmd: &HeadsetCommand, state: &HeadsetState) -> Result<Vec<(SettingId, Value)>> {
    Ok(match cmd {
        HeadsetCommand::SetAmbientMode { mode } => vec![(SettingId::AmbientSoundMode, Value::from(mode.as_lib()))],
        HeadsetCommand::SetNcMode { mode } => vec![(SettingId::NoiseCancelingMode, Value::from(mode.as_lib()))],
        HeadsetCommand::SetManualNc { level } => {
            vec![(SettingId::ManualNoiseCanceling, Value::I32(i32::from((*level).clamp(1, 5))))]
        }
        HeadsetCommand::SetTransparency { level } => {
            vec![(SettingId::ManualTransparency, Value::I32(i32::from((*level).clamp(1, 5))))]
        }
        HeadsetCommand::SetWindNoise { enabled } => vec![(SettingId::WindNoiseSuppression, Value::Bool(*enabled))],
        HeadsetCommand::SetEqPreset { preset } => {
            if !eq_of(state)?.presets.iter().any(|p| &p.id == preset) {
                return Err(AudioError::Invalid(format!("Unknown EQ preset \"{preset}\"")));
            }
            vec![(SettingId::PresetEqualizerProfile, Value::String(Cow::Owned(preset.clone())))]
        }
        HeadsetCommand::SetEqBands { values } => {
            let eq = eq_of(state)?;
            if values.len() != eq.band_hz.len() {
                return Err(AudioError::Invalid(format!("Expected {} EQ bands, got {}", eq.band_hz.len(), values.len())));
            }
            let clamped = values.iter().map(|v| (*v).clamp(eq.min, eq.max)).collect();
            vec![(SettingId::VolumeAdjustments, Value::I16Vec(clamped))]
        }
        HeadsetCommand::Retry => Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::headset::fixtures::space_one_settings;

    struct Map(HashMap<SettingId, Setting>);
    impl SettingSource for Map {
        fn setting(&self, id: &SettingId) -> Option<Setting> {
            self.0.get(id).cloned()
        }
    }

    fn state() -> HeadsetState {
        read_state(&Map(space_one_settings()), "soundcore Space One")
    }

    #[test]
    fn reads_every_supported_setting() {
        let s = state();
        assert_eq!(s.connection, Connection::Connected);
        assert_eq!(s.name.as_deref(), Some("soundcore Space One"));
        assert_eq!(s.ambient_mode, Some(AmbientMode::NoiseCanceling));
        assert_eq!(s.nc_mode, Some(NcMode::Manual));
        assert_eq!(s.manual_nc, Some(4));
        assert_eq!(s.adaptive_nc, Some(3));
        assert_eq!(s.transparency, Some(2));
        assert_eq!(s.wind_noise, Some(true));
        assert_eq!(s.battery, Some(Battery { level: 4, max: 5, charging: None }));
        assert_eq!(s.firmware.as_deref(), Some("06.87"));
        assert_eq!(s.serial.as_deref(), Some("3035705028ABCDEF"));
        let eq = s.eq.expect("eq");
        assert_eq!(eq.band_hz, vec![100, 200, 400, 800, 1600, 3200, 6400, 12800]);
        assert_eq!((eq.min, eq.max, eq.fraction_digits), (-120, 134, 1));
        assert_eq!(eq.values, vec![0; 8]);
        assert_eq!(eq.preset.as_deref(), Some("SoundcoreSignature"));
        assert_eq!(eq.presets[0], EqPreset { id: "SoundcoreSignature".into(), name: "Soundcore Signature".into() });
    }

    #[test]
    fn missing_or_unexpected_settings_become_none() {
        let mut m = space_one_settings();
        m.remove(&SettingId::BatteryLevel);
        m.insert(SettingId::AmbientSoundMode, serde_json::from_str(r#"{"type":"toggle","value":true}"#).unwrap());
        m.insert(SettingId::AdaptiveNoiseCanceling, serde_json::from_str(r#"{"type":"information","value":"n/a","translatedValue":"n/a"}"#).unwrap());
        let s = read_state(&Map(m), "x");
        assert_eq!(s.battery, None);
        assert_eq!(s.ambient_mode, None);
        assert_eq!(s.adaptive_nc, None);
        assert_eq!(read_state(&Map(HashMap::new()), "x").eq, None);
    }

    #[test]
    fn charging_is_read_when_the_device_reports_it() {
        let mut m = space_one_settings();
        m.insert(SettingId::IsCharging, serde_json::from_str(r#"{"type":"information","value":"true","translatedValue":"Charging"}"#).unwrap());
        assert_eq!(read_state(&Map(m), "x").battery.unwrap().charging, Some(true));
    }

    #[test]
    fn simple_commands_map_to_library_values() {
        let s = state();
        assert_eq!(
            command_values(&HeadsetCommand::SetAmbientMode { mode: AmbientMode::Transparency }, &s).unwrap(),
            vec![(SettingId::AmbientSoundMode, Value::from("Transparency"))]
        );
        assert_eq!(
            command_values(&HeadsetCommand::SetNcMode { mode: NcMode::Manual }, &s).unwrap(),
            vec![(SettingId::NoiseCancelingMode, Value::from("Custom"))]
        );
        assert_eq!(
            command_values(&HeadsetCommand::SetManualNc { level: 9 }, &s).unwrap(),
            vec![(SettingId::ManualNoiseCanceling, Value::I32(5))]
        );
        assert_eq!(
            command_values(&HeadsetCommand::SetTransparency { level: 0 }, &s).unwrap(),
            vec![(SettingId::ManualTransparency, Value::I32(1))]
        );
        assert_eq!(
            command_values(&HeadsetCommand::SetWindNoise { enabled: false }, &s).unwrap(),
            vec![(SettingId::WindNoiseSuppression, Value::Bool(false))]
        );
        assert!(command_values(&HeadsetCommand::Retry, &s).unwrap().is_empty());
    }

    #[test]
    fn eq_preset_must_be_known() {
        let s = state();
        assert_eq!(
            command_values(&HeadsetCommand::SetEqPreset { preset: "Acoustic".into() }, &s).unwrap(),
            vec![(SettingId::PresetEqualizerProfile, Value::from("Acoustic"))]
        );
        let err = command_values(&HeadsetCommand::SetEqPreset { preset: "Nope".into() }, &s).unwrap_err();
        assert_eq!(err.code(), "invalid");
    }

    #[test]
    fn eq_bands_are_clamped_and_counted() {
        let s = state();
        assert_eq!(
            command_values(&HeadsetCommand::SetEqBands { values: vec![-500, 0, 0, 0, 0, 0, 0, 500] }, &s).unwrap(),
            vec![(SettingId::VolumeAdjustments, Value::I16Vec(vec![-120, 0, 0, 0, 0, 0, 0, 134]))]
        );
        let err = command_values(&HeadsetCommand::SetEqBands { values: vec![0; 3] }, &s).unwrap_err();
        assert_eq!(err.code(), "invalid");
        let none = HeadsetState::with_connection(Connection::Connected);
        assert_eq!(command_values(&HeadsetCommand::SetEqBands { values: vec![0; 8] }, &none).unwrap_err().code(), "unavailable");
    }
}
