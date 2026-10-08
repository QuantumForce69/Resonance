use super::{coeff, db_to_lin};
use crate::engine::config::LimiterParams;

/// Look-ahead: 1.5 ms at 48 kHz.
pub const LOOKAHEAD: usize = 72;
/// Gain falls with a time constant of a quarter of the look-ahead (0.375 ms).
const ATTACK_MS: f32 = 0.375;

/// Samples the gain looks at: the look-ahead plus the sample leaving the delay line.
const WINDOW: usize = LOOKAHEAD + 1;

/// Minimum of the last `WINDOW` values in amortised O(1) per sample (a monotonic queue in fixed arrays,
/// so it never allocates).
struct WindowMin {
    at: [u64; WINDOW],
    value: [f32; WINDOW],
    head: usize,
    len: usize,
    now: u64,
}

impl WindowMin {
    fn new() -> Self {
        Self { at: [0; WINDOW], value: [1.0; WINDOW], head: 0, len: 0, now: 0 }
    }

    /// Adds `v` and returns the minimum of the window that ends with it.
    fn push(&mut self, v: f32) -> f32 {
        // Values no smaller than `v` can never be the minimum again while `v` is in the window.
        while self.len > 0 && self.value[(self.head + self.len - 1) % WINDOW] >= v {
            self.len -= 1;
        }
        let back = (self.head + self.len) % WINDOW;
        self.value[back] = v;
        self.at[back] = self.now;
        self.len += 1;
        if self.at[self.head] + WINDOW as u64 <= self.now {
            self.head = (self.head + 1) % WINDOW;
            self.len -= 1;
        }
        self.now += 1;
        self.value[self.head]
    }
}

/// Look-ahead peak limiter. Output never exceeds the ceiling: gain is lowered ahead of each peak, and
/// anything a fast transient still pushes over is clipped at the ceiling as a last resort.
pub struct Limiter {
    ceiling: f32,
    attack: f32,
    release: f32,
    delay: [f32; LOOKAHEAD],
    needed: WindowMin,
    pos: usize,
    gain: f32,
}

impl Limiter {
    pub fn new(p: &LimiterParams) -> Self {
        let mut l = Limiter {
            ceiling: 1.0,
            attack: coeff(ATTACK_MS),
            release: 0.0,
            delay: [0.0; LOOKAHEAD],
            needed: WindowMin::new(),
            pos: 0,
            gain: 1.0,
        };
        l.set(p);
        l
    }

    pub fn set(&mut self, p: &LimiterParams) {
        self.ceiling = db_to_lin(p.ceiling_db);
        self.release = coeff(p.release_ms);
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for x in buf {
            let input = *x;
            let level = input.abs();
            let need = if level > self.ceiling { self.ceiling / level } else { 1.0 };
            let delayed = self.delay[self.pos];
            self.delay[self.pos] = input;
            self.pos = (self.pos + 1) % LOOKAHEAD;
            let target = self.needed.push(need);
            let c = if target < self.gain { self.attack } else { self.release };
            self.gain = target + c * (self.gain - target);
            *x = (delayed * self.gain).clamp(-self.ceiling, self.ceiling);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dsp::{peak, test_signals::sine};

    fn run(p: &LimiterParams, input: &[f32]) -> Vec<f32> {
        let mut l = Limiter::new(p);
        let mut out = input.to_vec();
        for block in out.chunks_mut(480) {
            l.process(block);
        }
        out
    }

    #[test]
    fn window_minimum_matches_a_brute_force_scan() {
        let mut w = WindowMin::new();
        let mut history = Vec::new();
        let mut seed = 7u32;
        for _ in 0..5000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let v = (seed >> 8) as f32 / 16_777_216.0;
            history.push(v);
            let start = history.len().saturating_sub(WINDOW);
            let expected = history[start..].iter().fold(f32::MAX, |m, x| m.min(*x));
            assert_eq!(w.push(v), expected);
        }
    }

    #[test]
    fn full_scale_square_never_exceeds_the_ceiling() {
        let ceiling = db_to_lin(-1.0);
        let input: Vec<f32> = (0..48_000).map(|i| if (i / 100) % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let out = run(&LimiterParams::default(), &input);
        assert!(peak(&out) <= ceiling + 1e-6);
        assert!(out[1000..].iter().all(|x| x.abs() > ceiling * 0.98), "settled output sits at the ceiling");
    }

    #[test]
    fn a_single_spike_is_caught() {
        let mut input = vec![0.0f32; 4800];
        input[1000] = 1.0;
        input[1001] = -0.95;
        let out = run(&LimiterParams::default(), &input);
        assert!(peak(&out) <= db_to_lin(-1.0) + 1e-6);
    }

    #[test]
    fn audio_under_the_ceiling_passes_through_delayed() {
        let input = sine(440.0, -6.0, 0.2);
        let out = run(&LimiterParams::default(), &input);
        for i in 0..input.len() - LOOKAHEAD {
            assert!((out[i + LOOKAHEAD] - input[i]).abs() < 1e-6, "sample {i}");
        }
    }

    #[test]
    fn ceiling_follows_the_settings() {
        let input = sine(440.0, 0.0, 0.5);
        let out = run(&LimiterParams { ceiling_db: -6.0, ..Default::default() }, &input);
        assert!(peak(&out) <= db_to_lin(-6.0) + 1e-6);
    }
}
