use std::collections::HashMap;

use super::{clamp01, AppCore, SYSTEM_KEY};
use crate::{
    backend::AudioBackend,
    engine::SourceSpec,
    error::{AudioError, Result},
};

/// Process trees deeper than this are treated as unrelated (guards against PID-reuse loops).
const MAX_DEPTH: usize = 64;

/// Keeps only PIDs with no ancestor among `pids`: capturing a process includes its children, so a child
/// captured as well would be heard twice. If a parent loop removes every PID, the first one is kept.
pub fn root_pids(pids: &[u32], parents: &HashMap<u32, u32>) -> Vec<u32> {
    let has_listed_ancestor = |pid: u32| {
        let mut p = pid;
        for _ in 0..MAX_DEPTH {
            match parents.get(&p) {
                Some(&parent) if parent != 0 && parent != p => {
                    if pids.contains(&parent) {
                        return true;
                    }
                    p = parent;
                }
                _ => return false,
            }
        }
        false
    };
    let roots: Vec<u32> = pids.iter().copied().filter(|&pid| !has_listed_ancestor(pid)).collect();
    if roots.is_empty() {
        pids.first().copied().into_iter().collect()
    } else {
        roots
    }
}

impl<B: AudioBackend> AppCore<B> {
    /// The processes the call should hear now: every sent app that's playing, while the engine runs.
    fn desired_sources(&mut self) -> Vec<SourceSpec> {
        if self.engine_state.route.is_none() {
            return Vec::new();
        }
        let sent: Vec<(String, Vec<u32>, f32)> = self
            .state
            .apps
            .iter()
            .filter_map(|a| Some((a.key.clone(), a.pids.clone(), a.send?)))
            .filter(|(_, pids, _)| !pids.is_empty())
            .collect();
        let parents =
            if sent.iter().any(|(_, pids, _)| pids.len() > 1) { self.backend.process_parents() } else { HashMap::new() };
        sent.into_iter()
            .flat_map(|(key, pids, level)| {
                root_pids(&pids, &parents).into_iter().map(move |pid| SourceSpec { key: key.clone(), pid, level })
            })
            .collect()
    }

    /// Brings the engine's captured processes in line with the sends; capture problems become notices.
    pub(super) fn sync_sources(&mut self) {
        let want = self.desired_sources();
        if want != self.engine_state.sources {
            let problems = self.engine.set_sources(&want);
            self.notices.extend(problems);
            self.engine_state.sources = want;
        }
    }

    pub(super) fn set_app_send(&mut self, key: &str, level: Option<f32>) -> Result<()> {
        if key == SYSTEM_KEY {
            return Err(AudioError::Invalid("System sounds can't be sent into calls".into()));
        }
        match level {
            Some(v) => {
                let v = clamp01(v)?;
                self.settings.call_sends.insert(key.to_string(), v);
            }
            None => {
                self.settings.call_sends.remove(key);
            }
        }
        self.settings_dirty = true;
        let send = self.settings.call_sends.get(key).copied();
        self.patch_app(key, |a| a.send = send);
        self.sync_sources();
        Ok(())
    }
}
