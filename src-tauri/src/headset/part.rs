use serde::{Deserialize, Serialize};

use super::model::{AmbientMode, Connection, HeadsetCommand, HeadsetState, NcMode};

/// The Space One settings a profile restores.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HeadsetPart {
    pub ambient_mode: Option<AmbientMode>,
    pub nc_mode: Option<NcMode>,
    pub manual_nc: Option<u8>,
    pub transparency: Option<u8>,
    pub wind_noise: Option<bool>,
    /// An EQ preset id; `None` with `eq_bands` set means a custom curve.
    pub eq_preset: Option<String>,
    pub eq_bands: Option<Vec<i16>>,
}

impl HeadsetPart {
    /// The headphones' current settings, if they're connected.
    pub fn from_state(s: &HeadsetState) -> Option<Self> {
        if s.connection != Connection::Connected {
            return None;
        }
        Some(Self {
            ambient_mode: s.ambient_mode,
            nc_mode: s.nc_mode,
            manual_nc: s.manual_nc,
            transparency: s.transparency,
            wind_noise: s.wind_noise,
            eq_preset: s.eq.as_ref().and_then(|e| e.preset.clone()),
            eq_bands: s.eq.as_ref().filter(|e| e.preset.is_none()).map(|e| e.values.clone()),
        })
    }

    /// Commands that restore these settings: levels and settings first, then the noise-cancelling mode,
    /// then the ambient mode last, so a level change can't leave the headphones in the wrong mode.
    pub fn commands(&self) -> Vec<HeadsetCommand> {
        let mut out = Vec::new();
        if let Some(level) = self.manual_nc {
            out.push(HeadsetCommand::SetManualNc { level });
        }
        if let Some(level) = self.transparency {
            out.push(HeadsetCommand::SetTransparency { level });
        }
        if let Some(enabled) = self.wind_noise {
            out.push(HeadsetCommand::SetWindNoise { enabled });
        }
        match (&self.eq_preset, &self.eq_bands) {
            (Some(preset), _) => out.push(HeadsetCommand::SetEqPreset { preset: preset.clone() }),
            (None, Some(values)) => out.push(HeadsetCommand::SetEqBands { values: values.clone() }),
            (None, None) => {}
        }
        if let Some(mode) = self.nc_mode {
            out.push(HeadsetCommand::SetNcMode { mode });
        }
        if let Some(mode) = self.ambient_mode {
            out.push(HeadsetCommand::SetAmbientMode { mode });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headset::model::{Eq, EqPreset};

    fn connected() -> HeadsetState {
        let mut s = HeadsetState::with_connection(Connection::Connected);
        s.ambient_mode = Some(AmbientMode::NoiseCanceling);
        s.nc_mode = Some(NcMode::Manual);
        s.manual_nc = Some(4);
        s.transparency = Some(2);
        s.wind_noise = Some(true);
        s.eq = Some(Eq {
            presets: vec![EqPreset { id: "Acoustic".into(), name: "Acoustic".into() }],
            preset: Some("Acoustic".into()),
            band_hz: vec![100, 200],
            values: vec![10, -20],
            min: -120,
            max: 134,
            fraction_digits: 1,
        });
        s
    }

    #[test]
    fn captures_a_connected_headset_only() {
        let part = HeadsetPart::from_state(&connected()).unwrap();
        assert_eq!(part.ambient_mode, Some(AmbientMode::NoiseCanceling));
        assert_eq!((part.manual_nc, part.transparency, part.wind_noise), (Some(4), Some(2), Some(true)));
        assert_eq!((part.eq_preset.as_deref(), part.eq_bands.as_ref()), (Some("Acoustic"), None));
        assert!(HeadsetPart::from_state(&HeadsetState::with_connection(Connection::NotFound)).is_none());
    }

    #[test]
    fn a_custom_eq_is_saved_as_bands() {
        let mut s = connected();
        s.eq.as_mut().unwrap().preset = None;
        let part = HeadsetPart::from_state(&s).unwrap();
        assert_eq!((part.eq_preset, part.eq_bands), (None, Some(vec![10, -20])));
    }

    #[test]
    fn restores_levels_before_modes() {
        let part = HeadsetPart::from_state(&connected()).unwrap();
        assert_eq!(
            part.commands(),
            vec![
                HeadsetCommand::SetManualNc { level: 4 },
                HeadsetCommand::SetTransparency { level: 2 },
                HeadsetCommand::SetWindNoise { enabled: true },
                HeadsetCommand::SetEqPreset { preset: "Acoustic".into() },
                HeadsetCommand::SetNcMode { mode: NcMode::Manual },
                HeadsetCommand::SetAmbientMode { mode: AmbientMode::NoiseCanceling },
            ]
        );
        assert!(HeadsetPart::default().commands().is_empty());
    }
}
