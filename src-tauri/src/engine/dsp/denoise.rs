use nnnoiseless::DenoiseState;

use super::FRAME;

/// RNNoise returns each frame one frame late; the dry path is delayed to match.
pub const DENOISE_DELAY: usize = FRAME;
/// RNNoise works on 16-bit sample magnitudes.
const SCALE: f32 = 32_768.0;

/// AI noise suppression (RNNoise via nnnoiseless) with a wet/dry strength control.
pub struct Denoiser {
    state: Box<DenoiseState<'static>>,
    scaled: [f32; FRAME],
    wet: [f32; FRAME],
    dry: [f32; FRAME],
    strength: f32,
}

impl Denoiser {
    pub fn new(strength: f32) -> Self {
        Self { state: DenoiseState::new(), scaled: [0.0; FRAME], wet: [0.0; FRAME], dry: [0.0; FRAME], strength }
    }

    pub fn set_strength(&mut self, strength: f32) {
        self.strength = strength;
    }

    pub fn process(&mut self, frame: &mut [f32; FRAME]) {
        for (s, x) in self.scaled.iter_mut().zip(frame.iter()) {
            *s = x * SCALE;
        }
        self.state.process_frame(&mut self.wet, &self.scaled);
        for ((x, wet), dry) in frame.iter_mut().zip(self.wet.iter()).zip(self.dry.iter_mut()) {
            let wet = wet / SCALE;
            let delayed = std::mem::replace(dry, *x);
            *x = delayed + self.strength * (wet - delayed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::test_signals::{at, fan_noise, rms};
    use std::f32::consts::TAU;

    fn run(strength: f32, input: &[f32]) -> Vec<f32> {
        let mut d = Denoiser::new(strength);
        let mut out = input.to_vec();
        for f in out.chunks_exact_mut(FRAME) {
            d.process(f.try_into().unwrap());
        }
        out
    }

    #[test]
    fn removes_fan_noise() {
        let input = fan_noise(-40.0, 5.0);
        let out = run(1.0, &input);
        let (a, b) = (at(2.0), at(5.0));
        let reduction = 20.0 * (rms(&input[a..b]) / rms(&out[a..b]).max(1e-9)).log10();
        assert!(reduction > 15.0, "only {reduction:.1} dB");
    }

    #[test]
    fn keeps_a_voice_like_tone() {
        // Harmonic tone switched loud/quiet every 100 ms, like syllables.
        let input: Vec<f32> = (0..at(5.0))
            .map(|i| {
                let t = i as f32 / 48_000.0;
                let env = if (i / 4800) % 2 == 0 { 1.0 } else { 0.2 };
                env * 0.25 * ((TAU * 220.0 * t).sin() + 0.5 * (TAU * 440.0 * t).sin() + 0.3 * (TAU * 1250.0 * t).sin())
            })
            .collect();
        let out = run(1.0, &input);
        let (a, b) = (at(1.0), at(5.0));
        let kept = (rms(&out[a..b]) / rms(&input[a..b])).powi(2);
        assert!(kept > 0.8, "kept {kept:.2} of the energy");
    }

    #[test]
    fn zero_strength_is_the_input_one_frame_late() {
        let input = fan_noise(-30.0, 0.5);
        let out = run(0.0, &input);
        for i in 0..input.len() - DENOISE_DELAY {
            assert_eq!(out[i + DENOISE_DELAY], input[i], "sample {i}");
        }
    }

    #[test]
    fn wet_and_dry_paths_are_aligned() {
        let mut input = vec![0.0f32; FRAME * 20];
        for k in (4800..input.len()).step_by(960) {
            input[k] = 0.6;
        }
        let out = run(1.0, &input);
        let lag = out[4800..6000].iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).unwrap().0;
        assert_eq!(lag, DENOISE_DELAY);
    }
}
