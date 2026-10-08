use super::{coeff, db_to_lin, flush, SAMPLE_RATE};

const DETECT_RELEASE_MS: f32 = 20.0;
const ATTACK_MS: f32 = 10.0;
const HOLD_MS: f32 = 300.0;
const RELEASE_MS: f32 = 300.0;

/// Turns the user's processed voice into a gain for sent apps: down by the duck amount while they talk,
/// back up after a hold and a smooth release.
pub struct Ducker {
    detector: f32,
    detect_release: f32,
    attack: f32,
    release: f32,
    hold: u32,
    hold_left: u32,
    gain: f32,
}

impl Ducker {
    pub fn new() -> Self {
        Self {
            detector: 0.0,
            detect_release: coeff(DETECT_RELEASE_MS),
            attack: coeff(ATTACK_MS),
            release: coeff(RELEASE_MS),
            hold: (HOLD_MS * 0.001 * SAMPLE_RATE) as u32,
            hold_left: 0,
            gain: 1.0,
        }
    }

    /// Writes one gain per voice sample into `gains`. `duck_db: None` means ducking is off (gain 1).
    pub fn process(&mut self, voice: &[f32], threshold_db: f32, duck_db: Option<f32>, gains: &mut [f32]) {
        let threshold = db_to_lin(threshold_db);
        let ducked = duck_db.map_or(1.0, db_to_lin);
        for (x, g) in voice.iter().zip(gains.iter_mut()) {
            let level = x.abs();
            self.detector = if level > self.detector { level } else { flush(self.detector * self.detect_release) };
            if self.detector >= threshold {
                self.hold_left = self.hold;
            } else if self.hold_left > 0 {
                self.hold_left -= 1;
            }
            let talking = self.hold_left > 0;
            let (target, c) = if talking { (ducked, self.attack) } else { (1.0, self.release) };
            self.gain = target + flush(c * (self.gain - target));
            *g = self.gain;
        }
    }
}

impl Default for Ducker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::{
        lin_to_db,
        test_signals::{at, sine},
    };

    fn gains_for(voice: &[f32], duck_db: Option<f32>) -> Vec<f32> {
        let mut d = Ducker::new();
        let mut g = vec![0.0; voice.len()];
        for (v, out) in voice.chunks(480).zip(g.chunks_mut(480)) {
            d.process(v, -45.0, duck_db, out);
        }
        g
    }

    #[test]
    fn ducks_while_talking_and_recovers_after_hold_and_release() {
        let mut voice = vec![0.0f32; at(0.5)];
        voice.extend(sine(200.0, -20.0, 1.0));
        voice.extend(vec![0.0f32; at(2.0)]);
        let g = gains_for(&voice, Some(-12.0));
        assert!((g[at(0.49)] - 1.0).abs() < 1e-6, "no ducking before the voice");
        assert!((lin_to_db(g[at(0.55)]) + 12.0).abs() < 1.0, "ducked within 50 ms: {}", lin_to_db(g[at(0.55)]));
        assert!((lin_to_db(g[at(1.4)]) + 12.0).abs() < 0.1, "stays ducked while talking");
        assert!(lin_to_db(g[at(1.75)]) < -11.0, "held for 300 ms after the voice stops");
        assert!(lin_to_db(g[at(3.4)]) > -0.5, "recovered: {}", lin_to_db(g[at(3.4)]));
    }

    #[test]
    fn quiet_input_and_disabled_ducking_leave_gain_at_one() {
        let quiet = sine(200.0, -60.0, 1.0);
        assert!(gains_for(&quiet, Some(-12.0)).iter().all(|g| (g - 1.0).abs() < 1e-6));
        let loud = sine(200.0, -10.0, 1.0);
        assert!(gains_for(&loud, None).iter().all(|g| (g - 1.0).abs() < 1e-6));
    }
}
