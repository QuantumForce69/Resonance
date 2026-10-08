use super::{coeff, db_to_lin, flush};
use crate::engine::config::CompressorParams;

/// RMS detector time constant.
const RMS_MS: f32 = 10.0;
/// Soft-knee width around the threshold.
const KNEE_DB: f32 = 6.0;

/// Feed-forward RMS compressor with a soft knee and makeup gain.
pub struct Compressor {
    threshold: f32,
    slope: f32,
    makeup: f32,
    attack: f32,
    release: f32,
    rms: f32,
    mean_square: f32,
    reduction: f32,
}

impl Compressor {
    pub fn new(p: &CompressorParams) -> Self {
        let mut c = Compressor {
            threshold: 0.0,
            slope: 0.0,
            makeup: 0.0,
            attack: 0.0,
            release: 0.0,
            rms: coeff(RMS_MS),
            mean_square: 0.0,
            reduction: 0.0,
        };
        c.set(p);
        c
    }

    pub fn set(&mut self, p: &CompressorParams) {
        let (threshold, ratio) = p.curve();
        self.threshold = threshold;
        self.slope = 1.0 - 1.0 / ratio.max(1.0);
        self.makeup = p.makeup();
        self.attack = coeff(p.attack_ms);
        self.release = coeff(p.release_ms);
    }

    /// Static gain reduction (dB) for a detector level (dBFS).
    pub fn reduction_for(&self, level_db: f32) -> f32 {
        let over = level_db - self.threshold;
        if over <= -KNEE_DB / 2.0 {
            0.0
        } else if over < KNEE_DB / 2.0 {
            self.slope * (over + KNEE_DB / 2.0).powi(2) / (2.0 * KNEE_DB)
        } else {
            self.slope * over
        }
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            let sq = *x * *x;
            self.mean_square = sq + flush(self.rms * (self.mean_square - sq));
            let level_db = 10.0 * self.mean_square.max(1e-12).log10();
            let target = self.reduction_for(level_db);
            let c = if target > self.reduction { self.attack } else { self.release };
            self.reduction = target + flush(c * (self.reduction - target));
            *x *= db_to_lin(self.makeup - self.reduction);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::test_signals::{at, rms, sine};

    fn custom(threshold_db: f32, ratio: f32) -> CompressorParams {
        CompressorParams { custom: true, threshold_db, ratio, makeup_db: Some(0.0), ..Default::default() }
    }

    fn run(p: &CompressorParams, input: &[f32]) -> Vec<f32> {
        let mut c = Compressor::new(p);
        let mut out = input.to_vec();
        for block in out.chunks_mut(480) {
            c.process(block);
        }
        out
    }

    fn rms_db(x: &[f32]) -> f32 {
        20.0 * rms(x).log10()
    }

    #[test]
    fn reduces_level_above_threshold_by_the_ratio() {
        // Peak −7 dBFS = RMS −10 dBFS: 20 dB over a −30 threshold, 4:1 → 15 dB of reduction.
        let input = sine(220.0, -7.0, 1.0);
        let out = run(&custom(-30.0, 4.0), &input);
        let (a, b) = (at(0.5), at(1.0));
        let level_in = rms_db(&input[a..b]);
        let expected = level_in - 0.75 * (level_in + 30.0);
        let got = rms_db(&out[a..b]);
        assert!((got - expected).abs() < 0.5, "expected {expected:.2}, got {got:.2}");
    }

    #[test]
    fn leaves_quiet_signal_alone() {
        let input = sine(220.0, -40.0, 1.0);
        let out = run(&custom(-30.0, 4.0), &input);
        let (a, b) = (at(0.5), at(1.0));
        assert!((rms_db(&out[a..b]) - rms_db(&input[a..b])).abs() < 0.05);
    }

    #[test]
    fn applies_makeup_gain() {
        let input = sine(220.0, -40.0, 1.0);
        let out = run(&CompressorParams { makeup_db: Some(6.0), ..custom(-30.0, 4.0) }, &input);
        let (a, b) = (at(0.5), at(1.0));
        assert!((rms_db(&out[a..b]) - rms_db(&input[a..b]) - 6.0).abs() < 0.05);
    }

    #[test]
    fn soft_knee_is_continuous() {
        let c = Compressor::new(&custom(-30.0, 4.0));
        for edge in [-33.0f32, -27.0] {
            assert!((c.reduction_for(edge - 1e-3) - c.reduction_for(edge + 1e-3)).abs() < 1e-2);
        }
        assert_eq!(c.reduction_for(-40.0), 0.0);
        assert!((c.reduction_for(-10.0) - 15.0).abs() < 1e-4);
    }

    #[test]
    fn silence_leaves_no_denormals() {
        let mut c = Compressor::new(&CompressorParams::default());
        let mut loud = sine(220.0, -3.0, 0.1);
        c.process(&mut loud);
        let mut buf = vec![0.0f32; 480];
        for _ in 0..1000 {
            buf.fill(0.0);
            c.process(&mut buf);
        }
        assert!(buf.iter().all(|x| *x == 0.0));
        assert!(c.mean_square == 0.0 || c.mean_square.is_normal());
        assert!(c.reduction == 0.0 || c.reduction.is_normal());
    }
}
