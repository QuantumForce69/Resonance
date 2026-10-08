use rubato::{audioadapter_buffers::direct::InterleavedSlice, Adjustable, Async, FixedAsync, PolynomialDegree, Resampler};

use super::{drift::DriftController, dsp::FRAME, fifo::Fifo};

/// Frames kept queued for an output (its queue plus the device buffer): 20 ms rides out scheduling jitter.
pub const TARGET_QUEUE: usize = 960;
/// How far the resampling ratio may ever move from 1 (the drift controller stays within ±0.5 %).
const MAX_RATIO_RELATIVE: f64 = 1.1;

/// One output destination: resamples processed frames to follow the device's clock and queues them.
pub struct Feed {
    resampler: Async<f32>,
    fifo: Fifo,
    drift: DriftController,
    scratch: Vec<f32>,
}

impl Feed {
    pub fn new() -> Self {
        let resampler =
            Async::<f32>::new_poly(1.0, MAX_RATIO_RELATIVE, PolynomialDegree::Cubic, FRAME, 1, FixedAsync::Input)
                .expect("valid resampler settings");
        let scratch = vec![0.0; resampler.output_frames_max()];
        Self { resampler, fifo: Fifo::new(24_000), drift: DriftController::new(TARGET_QUEUE), scratch }
    }

    /// Resamples one processed block into the queue.
    pub fn push(&mut self, frame: &[f32; FRAME]) {
        let written = {
            let input = InterleavedSlice::new(&frame[..], 1, FRAME).expect("frame size");
            let len = self.scratch.len();
            let mut output = InterleavedSlice::new_mut(&mut self.scratch[..], 1, len).expect("scratch size");
            self.resampler.process_into_buffer(&input, &mut output, None).map(|(_, n)| n).unwrap_or(0)
        };
        self.fifo.push_slice(&self.scratch[..written]);
    }

    pub fn queued(&self) -> usize {
        self.fifo.len()
    }

    /// Takes up to `out.len()` queued frames.
    pub fn pull(&mut self, out: &mut [f32]) -> usize {
        self.fifo.pop_into(out)
    }

    /// Called once per cycle, after writing to the device, with the frames the device still holds.
    pub fn steer(&mut self, device_queued: usize) {
        let ratio = self.drift.update(self.fifo.len() + device_queued);
        let _ = self.resampler.set_resample_ratio(ratio, true);
    }
}

impl Default for Feed {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two minutes of 10 ms cycles against a device (prefilled with 20 ms of silence) whose clock runs
    /// `skew` fast. Returns the lowest and highest total queue after 30 s, and underruns.
    fn simulate(skew: f64) -> (usize, usize, usize) {
        let mut feed = Feed::new();
        let frame = [0.1f32; FRAME];
        let mut out = vec![0.0f32; 4800];
        let (mut device, mut played) = (TARGET_QUEUE as f64, 0.0f64);
        let (mut lo, mut hi, mut underruns) = (usize::MAX, 0, 0);
        for cycle in 0..12_000 {
            played += 480.0 * (1.0 + skew);
            let pull = played.floor();
            played -= pull;
            if device < pull {
                underruns += 1;
                device = 0.0;
            } else {
                device -= pull;
            }
            feed.push(&frame);
            device += feed.pull(&mut out) as f64;
            feed.steer(device as usize);
            if cycle >= 3_000 {
                let total = device as usize + feed.queued();
                lo = lo.min(total);
                hi = hi.max(total);
            }
        }
        (lo, hi, underruns)
    }

    #[test]
    fn follows_a_drifting_device_clock_without_underruns() {
        for skew in [-0.003, 0.0, 0.003] {
            let (lo, hi, underruns) = simulate(skew);
            assert_eq!(underruns, 0, "skew {skew}");
            assert!(lo > TARGET_QUEUE - 240 && hi < TARGET_QUEUE + 240, "skew {skew}: {lo}..{hi}");
        }
    }

    #[test]
    fn passes_audio_through_at_unity_ratio() {
        let mut feed = Feed::new();
        let frame = [0.25f32; FRAME];
        for _ in 0..10 {
            feed.push(&frame);
        }
        let mut out = vec![0.0f32; 4800];
        let n = feed.pull(&mut out);
        assert!((4790..=4800).contains(&n), "{n} frames");
        assert!(out[1000..n].iter().all(|x| (x - 0.25).abs() < 1e-4));
    }
}
