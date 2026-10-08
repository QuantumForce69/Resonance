mod group;
#[cfg(test)]
mod tests;
mod processing;
#[cfg(test)]
mod processing_tests;

pub use group::{app_key, group_sessions, SYSTEM_KEY};
pub use processing::{find_cable, Cable, RETRY_REFRESHES};

use std::collections::{HashMap, HashSet};

use crate::{
    backend::AudioBackend,
    engine::{AudioEngine, NullEngine},
    error::{AudioError, Result},
    model::*,
    settings::{AppPrefs, Settings},
};

/// Pure application logic on top of an `AudioBackend`. Owned by the audio worker thread.
pub struct AppCore<B: AudioBackend> {
    backend: B,
    settings: Settings,
    state: UiState,
    seen_sessions: HashSet<String>,
    settings_dirty: bool,
    engine: Box<dyn AudioEngine>,
    engine_state: processing::ProcState,
    notices: Vec<AudioError>,
}

fn clamp01(v: f32) -> Result<f32> {
    if v.is_finite() {
        Ok(v.clamp(0.0, 1.0))
    } else {
        Err(AudioError::Invalid(format!("Invalid volume {v}")))
    }
}

impl<B: AudioBackend> AppCore<B> {
    pub fn new(backend: B, settings: Settings) -> Self {
        let state = UiState {
            routing_available: backend.routing_available(),
            mic: settings.mic.clone(),
            ui: settings.ui.clone(),
            ..Default::default()
        };
let mut core = Self {
            backend,
            settings,
            state,
            seen_sessions: HashSet::new(),
            settings_dirty: false,
            engine: Box::new(NullEngine),
            engine_state: Default::default(),
            notices: Vec::new(),
        };
        core.state.processing = core.processing_view();
        core
    }

    pub fn state(&self) -> &UiState {
        &self.state
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn take_settings_dirty(&mut self) -> bool {
        std::mem::take(&mut self.settings_dirty)
    }

    /// Problems to show the user that didn't come from a command (e.g. a failed default-mic switch).
    pub fn take_notices(&mut self) -> Vec<AudioError> {
        std::mem::take(&mut self.notices)
    }

    #[cfg(test)]
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Re-reads the OS, restores saved prefs onto new sessions, remembers current values, and keeps the
    /// processing engine and default mics in line with the settings.
    pub fn refresh(&mut self) -> Result<bool> {
        let next = self.read_state()?;
        let previous = std::mem::replace(&mut self.state, next);
        if self.reconcile_processing() {
            self.state = self.read_state()?;
        }
        self.state.processing = self.processing_view();
        Ok(self.state != previous)
    }

    fn read_state(&mut self) -> Result<UiState> {
        let mut snap = self.backend.snapshot()?;
        if self.restore_new_sessions(&snap.sessions) {
            snap = self.backend.snapshot()?;
        }
        let apps = group_sessions(&snap.sessions);
        self.remember(&apps);
        Ok(self.build_state(snap.devices, apps))
    }

    fn restore_new_sessions(&mut self, sessions: &[RawSession]) -> bool {
        let mut applied = false;
        for s in sessions {
            if self.seen_sessions.contains(&s.id) {
                continue;
            }
            if let Some(p) = self.settings.apps.get(&app_key(s)).cloned() {
                if (s.volume - p.volume).abs() > 0.005 {
                    applied |= self.backend.set_session_volume(&s.id, p.volume).is_ok();
                }
                if s.muted != p.muted {
                    applied |= self.backend.set_session_mute(&s.id, p.muted).is_ok();
                }
            }
        }
        self.seen_sessions = sessions.iter().map(|s| s.id.clone()).collect();
        applied
    }

    fn remember(&mut self, apps: &[App]) {
        for a in apps.iter().filter(|a| !a.key.starts_with("pid:")) {
            let prefs = AppPrefs { volume: a.volume, muted: a.muted };
            if self.settings.apps.get(&a.key) != Some(&prefs) {
                self.settings.apps.insert(a.key.clone(), prefs);
                self.settings_dirty = true;
            }
        }
    }

    fn build_state(&self, raw: Vec<RawDevice>, mut apps: Vec<App>) -> UiState {
        for a in &mut apps {
            a.send = self.settings.call_sends.get(&a.key).copied();
        }
        let mut devices: Vec<Device> = raw
            .into_iter()
            .map(|d| {
                let prefs = self.settings.devices.get(&d.id).cloned().unwrap_or_default();
                Device {
                    name: prefs.alias.clone().unwrap_or_else(|| d.name.clone()),
                    original_name: d.name,
                    alias: prefs.alias,
                    hidden: prefs.hidden,
                    id: d.id,
                    flow: d.flow,
                    state: d.state,
                    default_console: d.default_console,
                    default_comms: d.default_comms,
                    volume: d.volume,
                    muted: d.muted,
                    boost: d.boost,
                }
            })
            .collect();
        devices.sort_by(|a, b| {
            (a.flow as u8)
                .cmp(&(b.flow as u8))
                .then((a.state as u8).cmp(&(b.state as u8)))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        UiState {
            devices,
            apps,
            routing_available: self.backend.routing_available(),
            mic: self.settings.mic.clone(),
            ui: self.settings.ui.clone(),
            processing: self.state.processing.clone(),
        }
    }

    fn app(&self, key: &str) -> Result<&App> {
        self.state
            .apps
            .iter()
            .find(|a| a.key == key)
            .ok_or_else(|| AudioError::NotFound(format!("App {key} isn't playing audio")))
    }

    fn device(&self, id: &str) -> Result<&Device> {
        self.state
            .devices
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(AudioError::device_gone)
    }

    fn default_mic(&self) -> Result<&Device> {
        self.state
            .devices
            .iter()
            .find(|d| d.flow == Flow::Capture && d.default_console && d.state == DeviceState::Active)
            .ok_or_else(|| AudioError::NotFound("No default microphone".into()))
    }

    pub fn mic_muted(&self) -> Option<bool> {
        self.default_mic().ok().map(|d| d.muted)
    }

    fn patch_app(&mut self, key: &str, f: impl Fn(&mut App)) {
        if let Some(a) = self.state.apps.iter_mut().find(|a| a.key == key) {
            f(a);
        }
    }

    fn patch_device(&mut self, id: &str, f: impl Fn(&mut Device)) {
        if let Some(d) = self.state.devices.iter_mut().find(|d| d.id == id) {
            f(d);
        }
    }

    fn reapply_device_prefs(&mut self) {
        for d in &mut self.state.devices {
            let p = self.settings.devices.get(&d.id).cloned().unwrap_or_default();
            d.name = p.alias.clone().unwrap_or_else(|| d.original_name.clone());
            d.alias = p.alias;
            d.hidden = p.hidden;
        }
    }

    pub fn apply(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::SetAppVolume { key, volume } => {
                let v = clamp01(volume)?;
                for sid in self.app(&key)?.session_ids.clone() {
                    self.backend.set_session_volume(&sid, v)?;
                }
                self.patch_app(&key, |a| a.volume = v);
            }
            Command::SetAppMute { key, muted } => {
                for sid in self.app(&key)?.session_ids.clone() {
                    self.backend.set_session_mute(&sid, muted)?;
                }
                self.patch_app(&key, |a| a.muted = muted);
            }
            Command::SetAppOutput { key, device_id } => {
                if !self.backend.routing_available() {
                    return Err(AudioError::Unavailable(
                        "Per-app output routing isn't available on this version of Windows".into(),
                    ));
                }
                if let Some(id) = &device_id {
                    self.device(id)?;
                }
                for pid in self.app(&key)?.pids.clone() {
                    self.backend.set_app_output(pid, device_id.as_deref())?;
                }
                self.refresh()?;
            }
            Command::SetDeviceVolume { id, volume } => {
                let v = clamp01(volume)?;
                self.device(&id)?;
                self.backend.set_device_volume(&id, v)?;
                self.patch_device(&id, |d| d.volume = v);
            }
            Command::SetDeviceMute { id, muted } => {
                self.device(&id)?;
                self.backend.set_device_mute(&id, muted)?;
                self.patch_device(&id, |d| d.muted = muted);
            }
            Command::SetDefaultDevice { id, role } => {
                self.device(&id)?;
                self.backend.set_default_device(&id, role)?;
                self.refresh()?;
            }
            Command::SetDeviceAlias { id, alias } => {
                let alias = alias.map(|a| a.trim().to_string()).filter(|a| !a.is_empty());
                self.settings.devices.entry(id).or_default().alias = alias;
                self.settings_dirty = true;
                self.reapply_device_prefs();
            }
            Command::SetDeviceHidden { id, hidden } => {
                self.settings.devices.entry(id).or_default().hidden = hidden;
                self.settings_dirty = true;
                self.reapply_device_prefs();
            }
            Command::SetMicBoost { id, db } => {
                let b = self
                    .device(&id)?
                    .boost
                    .clone()
                    .ok_or_else(AudioError::no_boost)?;
                let db = db.clamp(b.min, b.max);
                self.backend.set_mic_boost(&id, db)?;
                self.patch_device(&id, |d| {
                    if let Some(b) = d.boost.as_mut() {
                        b.db = db;
                    }
                });
            }
            Command::SetMicMute { muted } => {
                let id = self.default_mic()?.id.clone();
                self.backend.set_device_mute(&id, muted)?;
                self.patch_device(&id, |d| d.muted = muted);
            }
            Command::ToggleMicMute => {
                let (id, muted) = {
                    let d = self.default_mic()?;
                    (d.id.clone(), !d.muted)
                };
                self.backend.set_device_mute(&id, muted)?;
                self.patch_device(&id, |d| d.muted = muted);
            }
            Command::SetMicConfig { config } => {
                self.settings.mic = config.clone();
                self.state.mic = config;
                self.settings_dirty = true;
            }
            Command::SetUiSettings { ui } => {
                self.settings.ui = ui.clone();
                self.state.ui = ui;
                self.settings_dirty = true;
            }
            Command::ResetAppMemory => {
                self.settings.apps.clear();
                self.settings_dirty = true;
            }
            Command::SetAppSend { .. } => {
                return Err(AudioError::Unavailable("Sending apps into calls isn't available yet".into()));
            }
            Command::SetProcessing { processing } => {
                let mut p = processing;
                p.dsp = p.dsp.clamped();
                p.monitor_level = clamp01(p.monitor_level)?;
                self.settings.processing = p;
                self.settings_dirty = true;
                let params = self.engine_params();
                self.engine.set_params(&params);
                self.engine_state.retry_in = 0;
                if self.reconcile_processing() {
                    self.refresh()?;
                }
                self.state.processing = self.processing_view();
            }
        }
        Ok(())
    }

    pub fn meters(&mut self) -> MeterFrame {
        let mic_id = self.default_mic().ok().map(|d| d.id.clone());
        let peaks = self.backend.peaks(mic_id.as_deref());
        let apps: HashMap<String, f32> = self
            .state
            .apps
            .iter()
            .map(|a| {
                let p = a.session_ids.iter().filter_map(|s| peaks.sessions.get(s)).fold(0.0f32, |m, v| m.max(*v));
                (a.key.clone(), p)
            })
            .collect();
let (proc_in, proc_out) = self.engine.take_peaks();
        MeterFrame { apps, master: peaks.master, mic: peaks.mic, proc_in, proc_out }
    }

    pub fn stop_metering(&mut self) {
        self.backend.stop_metering();
    }
}
