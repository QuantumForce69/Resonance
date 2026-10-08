/// Largest speed correction applied to an output (±0.5 %, far beyond real clock drift).
pub const MAX_CORRECTION: f64 = 0.005;
const KP: f64 = 2e-6;
const KI: f64 = 2e-8;
/// Smoothing of the measured queue level per update.
const SMOOTHING: f64 = 0.02;

/// Keeps an output queue near its target level when the input and output devices run on slightly
/// different clocks, by steering the resampling ratio (output frames per input frame).
pub struct DriftController {
    target: f64,
    level: Option<f64>,
    integral: f64,
}

impl DriftController {
    pub fn new(target_frames: usize) -> Self {
        Self { target: target_frames as f64, level: None, integral: 0.0 }
    }

    /// Takes the frames currently queued for the device; returns the ratio to use next.
    pub fn update(&mut self, queued: usize) -> f64 {
        let q = queued as f64;
        let level = match self.level {
            Some(l) => l + SMOOTHING * (q - l),
            None => q,
        };
        self.level = Some(level);
        let error = self.target - level;
        self.integral = (self.integral + KI * error).clamp(-MAX_CORRECTION, MAX_CORRECTION);
        1.0 + (KP * error + self.integral).clamp(-MAX_CORRECTION, MAX_CORRECTION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ten minutes of 10 ms cycles against a device whose clock runs `skew` fast.
    /// Returns the queue's lowest and highest deviation from target after the first minute, and underruns.
    fn simulate(skew: f64) -> (f64, f64, usize) {
        let mut d = DriftController::new(960);
        let (mut queued, mut ratio, mut made, mut played) = (960.0f64, 1.0f64, 0.0f64, 0.0f64);
        let (mut lo, mut hi, mut underruns) = (f64::MAX, f64::MIN, 0);
        for cycle in 0..60_000 {
            played += 480.0 * (1.0 + skew);
            let pull = played.floor();
            played -= pull;
            if queued < pull {
                underruns += 1;
                queued = 0.0;
            } else {
                queued -= pull;
            }
            made += 480.0 * ratio;
            let push = made.floor();
            made -= push;
            queued += push;
            ratio = d.update(queued as usize);
            if cycle >= 6_000 {
                lo = lo.min(queued - 960.0);
                hi = hi.max(queued - 960.0);
            }
        }
        (lo, hi, underruns)
    }

    #[test]
    fn holds_the_queue_within_5_ms_for_clock_skew_up_to_0_3_percent() {
        for skew in [-0.003, -0.001, 0.0, 0.001, 0.003] {
            let (lo, hi, underruns) = simulate(skew);
            assert_eq!(underruns, 0, "skew {skew}");
            assert!(lo > -240.0 && hi < 240.0, "skew {skew}: {lo}..{hi}");
        }
    }

    #[test]
    fn correction_is_bounded() {
        let mut d = DriftController::new(960);
        for _ in 0..10_000 {
            assert!((d.update(0) - 1.0).abs() <= MAX_CORRECTION + 1e-12);
        }
        for _ in 0..10_000 {
            assert!((d.update(100_000) - 1.0).abs() <= MAX_CORRECTION + 1e-12);
        }
    }
}
