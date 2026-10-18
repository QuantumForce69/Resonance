//! Auto-restart (Voicemeeter's "restart when a device is disconnected"): remember the devices Resonance is
//! using; when one drops out, announce it; when it comes back, wait for its driver to settle, then restart
//! audio once. Pure, so it is unit-tested without a clock (one tick per worker refresh, 250 ms).

use std::collections::{HashMap, HashSet};

/// Refreshes to wait after a device returns before restarting (2 s at 250 ms).
pub const SETTLE_TICKS: u32 = 8;

#[derive(Default)]
pub struct Watchdog {
    /// Devices in use as of the last tick (present then), plus any that dropped out since, by id -> name.
    watched: HashMap<String, String>,
    /// Watched devices currently missing.
    missing: HashSet<String>,
    /// Ticks left before restarting, once something came back.
    countdown: Option<u32>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Tick {
    pub disconnected: Vec<String>,
    pub restart: bool,
}

impl Watchdog {
    pub fn tick(&mut self, enabled: bool, in_use: &[(String, String)], present: &HashSet<String>) -> Tick {
        if !enabled {
            *self = Self::default();
            return Tick::default();
        }
        let mut tick = Tick::default();
        for (id, name) in &self.watched {
            if !present.contains(id) && self.missing.insert(id.clone()) {
                tick.disconnected.push(name.clone());
            }
        }
        let returned: Vec<String> = self.missing.iter().filter(|id| present.contains(*id)).cloned().collect();
        if !returned.is_empty() {
            for id in &returned {
                self.missing.remove(id);
            }
            // A later return restarts the settling wait, so devices coming back together cause one restart.
            self.countdown = Some(SETTLE_TICKS);
        } else if let Some(n) = self.countdown {
            if n <= 1 {
                self.countdown = None;
                tick.restart = true;
            } else {
                self.countdown = Some(n - 1);
            }
        }
        // Watch what is in use now, plus devices that dropped out, so they are recognised when they return.
        let mut next: HashMap<String, String> =
            in_use.iter().filter(|(id, _)| present.contains(id)).cloned().collect();
        for (id, name) in &self.watched {
            if self.missing.contains(id) {
                next.entry(id.clone()).or_insert_with(|| name.clone());
            }
        }
        self.watched = next;
        tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    fn used(v: &[&str]) -> Vec<(String, String)> {
        v.iter().map(|s| (s.to_string(), format!("Dev {s}"))).collect()
    }

    #[test]
    fn a_device_dropping_out_is_announced_but_does_not_restart() {
        let mut w = Watchdog::default();
        w.tick(true, &used(&["mic"]), &ids(&["mic"]));
        let t = w.tick(true, &used(&[]), &ids(&[]));
        assert_eq!(t.disconnected, vec!["Dev mic".to_string()]);
        assert!(!t.restart);
    }

    #[test]
    fn a_returning_device_restarts_once_after_settling() {
        let mut w = Watchdog::default();
        w.tick(true, &used(&["mic"]), &ids(&["mic"]));
        w.tick(true, &used(&[]), &ids(&[]));
        let mut restarts = 0;
        for _ in 0..SETTLE_TICKS + 3 {
            if w.tick(true, &used(&["mic"]), &ids(&["mic"])).restart {
                restarts += 1;
            }
        }
        assert_eq!(restarts, 1);
    }

    #[test]
    fn several_devices_returning_together_restart_once() {
        let mut w = Watchdog::default();
        w.tick(true, &used(&["a", "b"]), &ids(&["a", "b"]));
        w.tick(true, &used(&[]), &ids(&[]));
        w.tick(true, &used(&["a"]), &ids(&["a"]));
        let mut restarts = 0;
        for _ in 0..SETTLE_TICKS + 3 {
            if w.tick(true, &used(&["a", "b"]), &ids(&["a", "b"])).restart {
                restarts += 1;
            }
        }
        assert_eq!(restarts, 1);
    }

    #[test]
    fn devices_not_in_use_are_ignored() {
        let mut w = Watchdog::default();
        w.tick(true, &used(&[]), &ids(&["other"]));
        let t = w.tick(true, &used(&[]), &ids(&[]));
        assert!(t.disconnected.is_empty());
    }

    #[test]
    fn a_device_no_longer_in_use_is_forgotten() {
        let mut w = Watchdog::default();
        w.tick(true, &used(&["headset"]), &ids(&["headset"]));
        // Still plugged in, but Windows now plays through the speakers.
        w.tick(true, &used(&["speakers"]), &ids(&["headset", "speakers"]));
        let t = w.tick(true, &used(&["speakers"]), &ids(&["speakers"]));
        assert!(t.disconnected.is_empty());
        for _ in 0..SETTLE_TICKS + 3 {
            let t = w.tick(true, &used(&["speakers"]), &ids(&["headset", "speakers"]));
            assert!(t.disconnected.is_empty() && !t.restart);
        }
    }

    #[test]
    fn nothing_happens_while_disabled() {
        let mut w = Watchdog::default();
        w.tick(false, &used(&["mic"]), &ids(&["mic"]));
        let t = w.tick(false, &used(&[]), &ids(&[]));
        assert!(t.disconnected.is_empty() && !t.restart);
        for _ in 0..SETTLE_TICKS + 2 {
            assert!(!w.tick(false, &used(&["mic"]), &ids(&["mic"])).restart);
        }
    }
}
