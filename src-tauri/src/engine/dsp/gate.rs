use super::{coeff, db_to_lin, flush, SAMPLE_RATE};
use crate::engine::config::GateParams;

/// The level detector falls this fast between peaks.
const DETECT_RELEASE_MS: f32 = 20.0;
/// The gate closes this far below the threshold, so levels hovering at it don't chatter.
const HYSTERESIS_DB: f32 = 6.0;

/// Noise gate: full level above the threshold; below it (after the hold time) attenuated by the range,
/// with smooth attack and release.
pub struct Gate {
    open_at: f32,
    close_at: f32,
    floor: f32,
    attack: f32,
    release: f32,
    hold: u32,
    detect_release: f32,
    detector: f32,
    gain: f32,
    hold_left: u32,
    open: bool,
}

impl Gate {
    pub fn new(p: &GateParams) -> Self {
        let mut g = Gate {
            open_at: 0.0,
            close_at: 0.0,
            floor: 0.0,
            attack: 0.0,
            release: 0.0,
            hold: 0,
            detect_release: coeff(DETECT_RELEASE_MS),
            detector: 0.0,
            gain: 0.0,
            hold_left: 0,
            open: false,
        };
        g.set(p);
        g.gain = g.floor;
        g
    }

    pub fn set(&mut self, p: &GateParams) {
        self.open_at = db_to_lin(p.threshold_db);
        self.close_at = db_to_lin(p.threshold_db - HYSTERESIS_DB);
        self.floor = db_to_lin(p.range_db);
        self.attack = coeff(p.attack_ms);
        self.release = coeff(p.release_ms);
        self.hold = (p.hold_ms * 0.001 * SAMPLE_RATE) as u32;
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            let level = x.abs();
            self.detector = if level > self.detector { level } else { flush(self.detector * self.detect_release) };
            if self.detector >= self.open_at {
                self.open = true;
                self.hold_left = self.hold;
            } else if self.detector < self.close_at {
                if self.hold_left > 0 {
                    self.hold_left -= 1;
                } else {
                    self.open = false;
                }
            }
            let (target, c) = if self.open { (1.0, self.attack) } else { (self.floor, self.release) };
            self.gain = target + flush(c * (self.gain - target));
            *x *= self.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::test_signals::{at, rms, sine};

    fn run(p: &GateParams, input: &[f32]) -> Vec<f32> {
        let mut g = Gate::new(p);
        let mut out = input.to_vec();
        for block in out.chunks_mut(480) {
            g.process(block);
        }
        out
    }

    fn ratio(input: &[f32], out: &[f32], from: f32, to: f32) -> f32 {
        let (a, b) = (at(from), at(to));
        rms(&out[a..b]) / rms(&input[a..b])
    }

    #[test]
    fn quiet_signal_is_attenuated_by_the_range() {
        let input = sine(220.0, -60.0, 1.0);
        let out = run(&GateParams::default(), &input);
        let r = ratio(&input, &out, 0.5, 1.0);
        let expected = db_to_lin(-60.0);
        assert!((r - expected).abs() < expected * 0.1, "ratio {r}");
    }

    #[test]
    fn range_sets_how_far_a_closed_gate_attenuates() {
        let input = sine(220.0, -60.0, 1.0);
        let out = run(&GateParams { range_db: -20.0, ..Default::default() }, &input);
        let r = ratio(&input, &out, 0.5, 1.0);
        assert!((r - 0.1).abs() < 0.005, "ratio {r}");
    }

    #[test]
    fn loud_signal_passes_unchanged() {
        let input = sine(220.0, -20.0, 1.0);
        let out = run(&GateParams::default(), &input);
        assert!((ratio(&input, &out, 0.1, 1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn holds_open_then_releases() {
        let mut input = sine(220.0, -20.0, 0.5);
        input.extend(sine(220.0, -60.0, 1.0));
        let out = run(&GateParams::default(), &input);
        assert!(ratio(&input, &out, 0.58, 0.60) > 0.95, "should still be open during the hold");
        assert!(ratio(&input, &out, 1.45, 1.5) < 0.01, "should have closed");
    }

    #[test]
    fn silence_leaves_no_denormals() {
        let mut g = Gate::new(&GateParams::default());
        let mut loud = sine(220.0, -10.0, 0.1);
        g.process(&mut loud);
        let mut buf = vec![0.0f32; 480];
        for _ in 0..1000 {
            buf.fill(0.0);
            g.process(&mut buf);
        }
        assert!(buf.iter().all(|x| *x == 0.0));
        assert!(g.detector == 0.0 || g.detector.is_normal());
        assert!(g.gain.is_normal());
    }
}
