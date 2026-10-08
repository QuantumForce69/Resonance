use serde::{Deserialize, Serialize};

/// `v` clamped to `lo..=hi`, or `fallback` if it isn't a number.
fn fit(v: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        fallback
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DenoiseParams {
    pub enabled: bool,
    /// 0 = original signal, 1 = fully denoised.
    pub strength: f32,
}

impl Default for DenoiseParams {
    fn default() -> Self {
        Self { enabled: true, strength: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GateParams {
    pub enabled: bool,
    pub threshold_db: f32,
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub release_ms: f32,
    /// Attenuation while the gate is closed.
    pub range_db: f32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self { enabled: true, threshold_db: -45.0, attack_ms: 2.0, hold_ms: 150.0, release_ms: 120.0, range_db: -60.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CompressorParams {
    pub enabled: bool,
    /// The main control (0..1); sets threshold and ratio unless `custom`.
    pub amount: f32,
    /// True once threshold or ratio were edited directly.
    pub custom: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    /// `None` = automatic makeup gain.
    pub makeup_db: Option<f32>,
}

impl Default for CompressorParams {
    fn default() -> Self {
        Self {
            enabled: true,
            amount: 0.5,
            custom: false,
            threshold_db: -22.5,
            ratio: 3.75,
            attack_ms: 5.0,
            release_ms: 80.0,
            makeup_db: None,
        }
    }
}

impl CompressorParams {
    /// Threshold (dBFS) and ratio for an Amount: 0 → −10 dBFS at 1.5:1, 1 → −35 dBFS at 6:1.
    pub fn from_amount(amount: f32) -> (f32, f32) {
        let a = amount.clamp(0.0, 1.0);
        (-10.0 - 25.0 * a, 1.5 + 4.5 * a)
    }

    /// The threshold and ratio in effect.
    pub fn curve(&self) -> (f32, f32) {
        if self.custom {
            (self.threshold_db, self.ratio)
        } else {
            Self::from_amount(self.amount)
        }
    }

    /// Makeup gain in dB: the explicit value, or half the gain reduction a −18 dBFS signal receives.
    pub fn makeup(&self) -> f32 {
        self.makeup_db.unwrap_or_else(|| {
            let (threshold, ratio) = self.curve();
            (-18.0 - threshold).max(0.0) * (1.0 - 1.0 / ratio) / 2.0
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LimiterParams {
    pub enabled: bool,
    pub ceiling_db: f32,
    pub release_ms: f32,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self { enabled: true, ceiling_db: -1.0, release_ms: 50.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DspParams {
    pub denoise: DenoiseParams,
    pub gate: GateParams,
    pub compressor: CompressorParams,
    pub limiter: LimiterParams,
}

impl DspParams {
    /// Every parameter forced into its allowed range (spec §6); non-numbers fall back to defaults.
    pub fn clamped(&self) -> Self {
        let d = DspParams::default();
        let mut p = *self;
        p.denoise.strength = fit(p.denoise.strength, 0.0, 1.0, d.denoise.strength);
        let g = &mut p.gate;
        g.threshold_db = fit(g.threshold_db, -80.0, -10.0, d.gate.threshold_db);
        g.attack_ms = fit(g.attack_ms, 0.5, 50.0, d.gate.attack_ms);
        g.hold_ms = fit(g.hold_ms, 0.0, 1000.0, d.gate.hold_ms);
        g.release_ms = fit(g.release_ms, 10.0, 1000.0, d.gate.release_ms);
        g.range_db = fit(g.range_db, -80.0, 0.0, d.gate.range_db);
        let c = &mut p.compressor;
        c.amount = fit(c.amount, 0.0, 1.0, d.compressor.amount);
        c.threshold_db = fit(c.threshold_db, -60.0, 0.0, d.compressor.threshold_db);
        c.ratio = fit(c.ratio, 1.0, 20.0, d.compressor.ratio);
        c.attack_ms = fit(c.attack_ms, 0.1, 100.0, d.compressor.attack_ms);
        c.release_ms = fit(c.release_ms, 10.0, 1000.0, d.compressor.release_ms);
        c.makeup_db = c.makeup_db.map(|m| fit(m, -12.0, 24.0, 0.0));
        let l = &mut p.limiter;
        l.ceiling_db = fit(l.ceiling_db, -12.0, 0.0, d.limiter.ceiling_db);
        l.release_ms = fit(l.release_ms, 5.0, 500.0, d.limiter.release_ms);
        p
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProcessingSettings {
    pub enabled: bool,
    /// Capture device to process; `None` = the mic that was the Windows default before processing.
    pub source_id: Option<String>,
    pub dsp: DspParams,
    /// Play the processed mic on the default output.
    pub monitor: bool,
    pub monitor_level: f32,
}

impl Default for ProcessingSettings {
    fn default() -> Self {
        Self { enabled: false, source_id: None, dsp: DspParams::default(), monitor: false, monitor_level: 0.7 }
    }
}

/// The Windows default mics to put back when processing is turned off.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RestoreDefaults {
    pub console: Option<String>,
    pub comms: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EngineStatus {
    #[default]
    Off,
    NoCable,
    Running,
    Paused { reason: String },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingView {
    pub settings: ProcessingSettings,
    pub status: EngineStatus,
    pub cable_installed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_spec() {
        let d = DspParams::default();
        assert!(d.denoise.enabled && d.gate.enabled && d.compressor.enabled && d.limiter.enabled);
        assert_eq!(d.denoise.strength, 1.0);
        let g = d.gate;
        assert_eq!((g.threshold_db, g.attack_ms, g.hold_ms, g.release_ms, g.range_db), (-45.0, 2.0, 150.0, 120.0, -60.0));
        assert_eq!(d.compressor.curve(), (-22.5, 3.75));
        assert_eq!((d.limiter.ceiling_db, d.limiter.release_ms), (-1.0, 50.0));
        let s = ProcessingSettings::default();
        assert!(!s.enabled && !s.monitor);
        assert_eq!(s.monitor_level, 0.7);
    }

    #[test]
    fn amount_sets_threshold_and_ratio_until_custom() {
        assert_eq!(CompressorParams::from_amount(0.0), (-10.0, 1.5));
        assert_eq!(CompressorParams::from_amount(1.0), (-35.0, 6.0));
        let mut c = CompressorParams { custom: true, threshold_db: -30.0, ratio: 8.0, ..Default::default() };
        assert_eq!(c.curve(), (-30.0, 8.0));
        c.custom = false;
        assert_eq!(c.curve(), (-22.5, 3.75));
    }

    #[test]
    fn auto_makeup_is_half_the_reduction_at_minus_18() {
        // 12 dB over a −30 threshold at 4:1 reduces by 9 dB; half of that is 4.5.
        let c = CompressorParams { custom: true, threshold_db: -30.0, ratio: 4.0, ..Default::default() };
        assert!((c.makeup() - 4.5).abs() < 1e-5);
        let c = CompressorParams { custom: true, threshold_db: -10.0, ratio: 4.0, ..Default::default() };
        assert_eq!(c.makeup(), 0.0);
        let c = CompressorParams { makeup_db: Some(3.0), ..Default::default() };
        assert_eq!(c.makeup(), 3.0);
    }

    #[test]
    fn clamped_forces_ranges_and_replaces_non_numbers() {
        let mut p = DspParams::default();
        p.denoise.strength = -1.0;
        p.gate.threshold_db = -200.0;
        p.gate.hold_ms = f32::NAN;
        p.compressor.ratio = 50.0;
        p.compressor.makeup_db = Some(99.0);
        p.limiter.ceiling_db = 3.0;
        let c = p.clamped();
        assert_eq!(c.denoise.strength, 0.0);
        assert_eq!(c.gate.threshold_db, -80.0);
        assert_eq!(c.gate.hold_ms, 150.0);
        assert_eq!(c.compressor.ratio, 20.0);
        assert_eq!(c.compressor.makeup_db, Some(24.0));
        assert_eq!(c.limiter.ceiling_db, 0.0);
    }

    #[test]
    fn serializes_camel_case_with_a_tagged_status() {
        let view = ProcessingView {
            status: EngineStatus::Paused { reason: "gone".into() },
            cable_installed: true,
            ..Default::default()
        };
        let v = serde_json::to_value(&view).unwrap();
        assert_eq!(v["status"]["state"], "paused");
        assert_eq!(v["status"]["reason"], "gone");
        assert_eq!(v["cableInstalled"], true);
        assert!(v["settings"]["sourceId"].is_null());
        assert_eq!(v["settings"]["dsp"]["gate"]["thresholdDb"], -45.0);
        assert!(v["settings"]["dsp"]["compressor"]["makeupDb"].is_null());
        assert_eq!(serde_json::to_value(EngineStatus::NoCable).unwrap()["state"], "noCable");
    }

    #[test]
    fn partial_json_fills_in_defaults() {
        let p: ProcessingSettings =
            serde_json::from_str(r#"{"enabled":true,"dsp":{"gate":{"thresholdDb":-30}}}"#).unwrap();
        assert!(p.enabled);
        assert_eq!(p.dsp.gate.threshold_db, -30.0);
        assert_eq!(p.dsp.gate.attack_ms, 2.0);
        assert_eq!(p.dsp.denoise, DenoiseParams::default());
    }
}
