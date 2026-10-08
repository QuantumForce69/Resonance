use super::{ducker::Ducker, FRAME};
use crate::engine::config::CallSettings;

/// Sums sent apps for one block, then applies the master level and ducking.
pub struct CallBus {
    bus: [f32; FRAME],
    gains: [f32; FRAME],
    ducker: Ducker,
    active: bool,
}

impl CallBus {
    pub fn new() -> Self {
        Self { bus: [0.0; FRAME], gains: [1.0; FRAME], ducker: Ducker::new(), active: false }
    }

    /// Starts a new block.
    pub fn clear(&mut self) {
        self.bus.fill(0.0);
        self.active = false;
    }

    /// Adds one source's block at `level`.
    pub fn add(&mut self, src: &[f32; FRAME], level: f32) {
        for (b, s) in self.bus.iter_mut().zip(src) {
            *b += s * level;
        }
        self.active = true;
    }

    /// Applies master level and ducking (driven by the processed `voice`). Returns the bus if any source
    /// was added this block. The ducker runs every block so its state stays continuous.
    pub fn finish(&mut self, voice: &[f32; FRAME], call: &CallSettings, threshold_db: f32) -> Option<&[f32; FRAME]> {
        self.ducker.process(voice, threshold_db, call.duck.then_some(call.duck_db), &mut self.gains);
        if !self.active {
            return None;
        }
        for (b, g) in self.bus.iter_mut().zip(&self.gains) {
            *b *= call.master * g;
        }
        Some(&self.bus)
    }
}

impl Default for CallBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SILENT: [f32; FRAME] = [0.0; FRAME];

    #[test]
    fn nothing_sent_means_no_bus() {
        let mut bus = CallBus::new();
        bus.clear();
        assert!(bus.finish(&SILENT, &CallSettings::default(), -45.0).is_none());
    }

    #[test]
    fn sums_sources_at_their_levels_and_the_master() {
        let mut bus = CallBus::new();
        bus.clear();
        bus.add(&[0.5; FRAME], 0.5);
        bus.add(&[0.2; FRAME], 1.0);
        let call = CallSettings { master: 0.5, ..Default::default() };
        let out = bus.finish(&SILENT, &call, -45.0).unwrap();
        assert!(out.iter().all(|x| (x - 0.225).abs() < 1e-6), "{}", out[0]);
    }

    #[test]
    fn voice_ducks_the_bus() {
        let mut bus = CallBus::new();
        let voice = [0.5f32; FRAME];
        let call = CallSettings { duck_db: -20.0, ..Default::default() };
        let mut last = 0.0;
        for _ in 0..20 {
            bus.clear();
            bus.add(&[1.0; FRAME], 1.0);
            last = bus.finish(&voice, &call, -45.0).unwrap()[FRAME - 1];
        }
        assert!((last - 0.1).abs() < 0.005, "{last}");
    }
}
