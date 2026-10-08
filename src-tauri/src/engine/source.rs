use super::{
    dsp::FRAME,
    feed::{Feed, TARGET_QUEUE},
    fifo::Fifo,
};

/// One sent app on its way into the call: captured audio (on the app's playback clock) is resampled to
/// follow the mic clock. Output starts once `TARGET_QUEUE` frames are queued and pauses (silence) whenever
/// the queue runs dry, so jitter never chops the sound.
pub struct SourceQueue {
    raw: Fifo,
    block: [f32; FRAME],
    feed: Feed,
    primed: bool,
}

impl SourceQueue {
    pub fn new() -> Self {
        Self { raw: Fifo::new(48_000), block: [0.0; FRAME], feed: Feed::new(), primed: false }
    }

    /// Adds captured mono audio.
    pub fn push(&mut self, mono: &[f32]) {
        self.raw.push_slice(mono);
        while self.raw.len() >= FRAME {
            self.raw.pop_into(&mut self.block);
            self.feed.push(&self.block);
        }
    }

    /// Takes one engine block; anything missing is silence.
    pub fn pull(&mut self, out: &mut [f32; FRAME]) {
        if !self.primed {
            if self.feed.queued() < TARGET_QUEUE {
                out.fill(0.0);
                return;
            }
            self.primed = true;
        }
        let got = self.feed.pull(out);
        out[got..].fill(0.0);
        if got < FRAME {
            self.primed = false;
        } else {
            self.feed.steer(0);
        }
    }
}

impl Default for SourceQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_until_primed_then_continuous() {
        let mut q = SourceQueue::new();
        let mut out = [1.0f32; FRAME];
        q.pull(&mut out);
        assert!(out.iter().all(|x| *x == 0.0));
        q.push(&[0.5; 3 * FRAME]);
        q.pull(&mut out);
        assert!(out[FRAME / 2..].iter().all(|x| (x - 0.5).abs() < 1e-3));
    }

    #[test]
    fn re_primes_after_running_dry() {
        let mut q = SourceQueue::new();
        let mut out = [0.0f32; FRAME];
        q.push(&[0.5; 3 * FRAME]);
        for _ in 0..10 {
            q.pull(&mut out);
        }
        assert!(out.iter().all(|x| *x == 0.0), "dry → silence");
        q.push(&[0.5; FRAME]);
        q.pull(&mut out);
        assert!(out.iter().all(|x| *x == 0.0), "not primed again yet");
    }

    /// Two minutes of 10 ms cycles: the app delivers 480 × (1 + skew) frames per cycle in uneven packets.
    fn simulate(skew: f64) -> (usize, usize, usize) {
        let mut q = SourceQueue::new();
        let mut out = [0.0f32; FRAME];
        let (mut made, mut partial, mut lo, mut hi) = (0.0f64, 0, usize::MAX, 0);
        let chunk = vec![0.25f32; 2000];
        for cycle in 0..12_000 {
            made += 480.0 * (1.0 + skew);
            let n = made.floor() as usize;
            made -= n as f64;
            let split = n / 3;
            q.push(&chunk[..split]);
            q.push(&chunk[..n - split]);
            q.pull(&mut out);
            if cycle >= 3_000 {
                if out[FRAME - 1] == 0.0 {
                    partial += 1;
                }
                let level = q.feed.queued();
                lo = lo.min(level);
                hi = hi.max(level);
            }
        }
        (lo, hi, partial)
    }

    #[test]
    fn follows_the_mic_clock_without_gaps() {
        for skew in [-0.003, 0.0, 0.003] {
            let (lo, hi, partial) = simulate(skew);
            assert_eq!(partial, 0, "skew {skew}");
            assert!(lo > TARGET_QUEUE - 240 - FRAME && hi < TARGET_QUEUE + 240 + FRAME, "skew {skew}: {lo}..{hi}");
        }
    }
}
