//! Allocation-free processing of 48 kHz mono f32 audio.
pub mod callbus;
pub mod chain;
pub mod compressor;
pub mod denoise;
pub mod ducker;
pub mod gate;
pub mod limiter;

pub const SAMPLE_RATE: f32 = 48_000.0;
/// Samples per processing block: 10 ms, RNNoise's frame size.
pub const FRAME: usize = 480;

pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn lin_to_db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// Per-sample coefficient of a one-pole smoother with time constant `ms` (0 = instant).
pub fn coeff(ms: f32) -> f32 {
    if ms <= 0.0 {
        0.0
    } else {
        (-1.0 / (ms * 0.001 * SAMPLE_RATE)).exp()
    }
}

/// Values this small become zero, so long silences can't leave slow denormal numbers behind.
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-15 {
        0.0
    } else {
        x
    }
}

pub fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0, |m, x| m.max(x.abs()))
}

#[cfg(test)]
pub mod test_signals {
    use super::{db_to_lin, SAMPLE_RATE};

    /// Sine at `freq` Hz with a peak of `db` dBFS.
    pub fn sine(freq: f32, db: f32, seconds: f32) -> Vec<f32> {
        let a = db_to_lin(db);
        (0..at(seconds)).map(|i| a * (std::f32::consts::TAU * freq * i as f32 / SAMPLE_RATE).sin()).collect()
    }

    pub fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|x| x * x).sum::<f32>() / buf.len().max(1) as f32).sqrt()
    }

    /// Index of the sample `seconds` into a signal.
    pub fn at(seconds: f32) -> usize {
        (seconds * SAMPLE_RATE) as usize
    }

    /// Low-passed noise, like a fan or air conditioner, at `db` dBFS RMS.
    pub fn fan_noise(db: f32, seconds: f32) -> Vec<f32> {
        let mut seed = 0x1234_5678u32;
        let mut lp = 0.0f32;
        let raw: Vec<f32> = (0..at(seconds))
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let white = (seed >> 8) as f32 / 16_777_216.0 - 0.5;
                lp += 0.1 * (white - lp);
                lp
            })
            .collect();
        let gain = db_to_lin(db) / rms(&raw);
        raw.into_iter().map(|x| x * gain).collect()
    }
}
