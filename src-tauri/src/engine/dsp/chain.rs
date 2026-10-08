use super::{compressor::Compressor, denoise::Denoiser, gate::Gate, limiter::Limiter, peak, FRAME};
use crate::engine::config::{DspParams, LimiterParams};

/// The processing chain, in its fixed order: noise suppression → gate → compressor → limiter.
pub struct Chain {
    p: DspParams,
    denoise: Denoiser,
    gate: Gate,
    compressor: Compressor,
    limiter: Limiter,
    safety: Limiter,
}

impl Chain {
    pub fn new(p: &DspParams) -> Self {
        Self {
            p: *p,
            denoise: Denoiser::new(p.denoise.strength),
            gate: Gate::new(&p.gate),
            compressor: Compressor::new(&p.compressor),
            limiter: Limiter::new(&p.limiter),
            safety: Limiter::new(&LimiterParams::default()),
        }
    }

    /// Applies new settings without resetting any stage (no clicks, no allocation).
    pub fn set(&mut self, p: &DspParams) {
        self.denoise.set_strength(p.denoise.strength);
        self.gate.set(&p.gate);
        self.compressor.set(&p.compressor);
        self.limiter.set(&p.limiter);
        self.p = *p;
    }

    /// The voice path: noise suppression, gate and compressor. Returns the block's peak before processing.
    pub fn voice(&mut self, frame: &mut [f32; FRAME]) -> f32 {
        let before = peak(frame);
        if self.p.denoise.enabled {
            self.denoise.process(frame);
        }
        if self.p.gate.enabled {
            self.gate.process(frame);
        }
        if self.p.compressor.enabled {
            self.compressor.process(frame);
        }
        before
    }

    /// Adds the call bus (if any) and limits. With apps mixed in and the limiter stage off, a fixed −1 dBFS
    /// safety limiter still protects the call. Returns the block's peak after.
    pub fn finish(&mut self, frame: &mut [f32; FRAME], apps: Option<&[f32; FRAME]>) -> f32 {
        if let Some(apps) = apps {
            for (x, a) in frame.iter_mut().zip(apps) {
                *x += a;
            }
        }
        if self.p.limiter.enabled {
            self.limiter.process(frame);
        } else if apps.is_some() {
            self.safety.process(frame);
        }
        peak(frame)
    }

    /// The voice level that counts as talking, for ducking: the gate threshold, or −45 dBFS without a gate.
    pub fn duck_threshold_db(&self) -> f32 {
        if self.p.gate.enabled {
            self.p.gate.threshold_db
        } else {
            -45.0
        }
    }

    /// Voice path and limiter with no apps; returns the peaks before and after.
    pub fn process(&mut self, frame: &mut [f32; FRAME]) -> (f32, f32) {
        let before = self.voice(frame);
        (before, self.finish(frame, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::{
        db_to_lin,
        test_signals::{fan_noise, sine},
    };

    fn all_off() -> DspParams {
        let mut p = DspParams::default();
        p.denoise.enabled = false;
        p.gate.enabled = false;
        p.compressor.enabled = false;
        p.limiter.enabled = false;
        p
    }

    fn run(chain: &mut Chain, input: &[f32]) -> Vec<f32> {
        let mut out = input.to_vec();
        for f in out.as_chunks_mut::<FRAME>().0 {
            chain.process(f);
        }
        out
    }

    #[test]
    fn bypassed_chain_is_bit_exact() {
        let input = fan_noise(-20.0, 0.5);
        assert_eq!(run(&mut Chain::new(&all_off()), &input), input);
    }

    #[test]
    fn each_stage_works_on_its_own() {
        let input = sine(220.0, -20.0, 0.5);
        for stage in 0..4 {
            let mut p = all_off();
            match stage {
                0 => p.denoise.enabled = true,
                1 => p.gate.enabled = true,
                2 => p.compressor.enabled = true,
                _ => p.limiter.enabled = true,
            }
            assert_ne!(run(&mut Chain::new(&p), &input), input, "stage {stage} had no effect");
        }
    }

    #[test]
    fn reports_peaks_before_and_after() {
        let mut p = all_off();
        p.limiter.enabled = true;
        p.limiter.ceiling_db = -6.0;
        let mut chain = Chain::new(&p);
        let mut last = (0.0, 0.0);
        for _ in 0..10 {
            let mut frame = [0.0f32; FRAME];
            for (i, x) in frame.iter_mut().enumerate() {
                *x = if i % 2 == 0 { 0.9 } else { -0.9 };
            }
            last = chain.process(&mut frame);
        }
        assert!((last.0 - 0.9).abs() < 1e-6);
        assert!(last.1 <= db_to_lin(-6.0) + 1e-6);
    }

    #[test]
    fn settings_change_live() {
        let mut p = all_off();
        p.gate.enabled = true;
        p.gate.threshold_db = -10.0;
        let mut chain = Chain::new(&p);
        let input = sine(220.0, -30.0, 0.5);
        let closed = run(&mut chain, &input);
        assert!(crate::engine::dsp::peak(&closed[12_000..]) < 0.001);
        p.gate.threshold_db = -45.0;
        chain.set(&p);
        let open = run(&mut chain, &input);
        assert!((crate::engine::dsp::peak(&open[12_000..]) - db_to_lin(-30.0)).abs() < 0.002);
    }

    #[test]
    fn full_chain_stays_under_the_ceiling() {
        let loud: Vec<f32> = sine(180.0, 0.0, 1.0).iter().zip(fan_noise(-30.0, 1.0)).map(|(s, n)| s + n).collect();
        let out = run(&mut Chain::new(&DspParams::default()), &loud);
        assert!(crate::engine::dsp::peak(&out) <= db_to_lin(-1.0) + 1e-6);
    }

    #[test]
    fn apps_are_added_after_the_voice_path_and_limited() {
        let mut p = all_off();
        p.gate.enabled = true;
        p.gate.threshold_db = -10.0; // gate shuts the quiet "mic"
        let mut chain = Chain::new(&p);
        let apps = [0.9f32; FRAME];
        let mut out = 0.0;
        for _ in 0..20 {
            let mut frame = [0.001f32; FRAME];
            chain.voice(&mut frame);
            out = chain.finish(&mut frame, Some(&apps));
        }
        // The gate didn't touch the apps; the safety limiter held them at −1 dBFS.
        assert!((out - db_to_lin(-1.0)).abs() < 0.01, "{out}");
    }

    #[test]
    fn without_apps_and_limiter_nothing_is_limited() {
        let mut chain = Chain::new(&all_off());
        let mut frame = [0.99f32; FRAME];
        chain.voice(&mut frame);
        assert_eq!(chain.finish(&mut frame, None), 0.99);
    }
}
