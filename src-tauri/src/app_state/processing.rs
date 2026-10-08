use super::AppCore;
use crate::{
    backend::AudioBackend,
    engine::{AudioEngine, EngineParams, EngineStatus, ProcessingView, RestoreDefaults, Route},
    error::AudioError,
    model::{Device, DeviceState, Flow, Role},
};

/// Windows names every VB-Cable endpoint "<endpoint> (VB-Audio Virtual Cable)".
const CABLE_DEVICE: &str = "(VB-Audio Virtual Cable)";
const CABLE_RENDER: &str = "CABLE Input";
const CABLE_CAPTURE: &str = "CABLE Output";
/// After the engine fails to start or stops by itself, wait this many refreshes (2 s) before retrying.
pub const RETRY_REFRESHES: u32 = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Cable {
    pub render_id: String,
    pub capture_id: String,
}

/// VB-Cable's endpoints, matched by their Windows names (aliases don't matter). The playback side is
/// "CABLE Input" in most versions, but newer driver packs add "CABLE In 16 Ch" and can show the stereo
/// input as "Speakers (VB-Audio Virtual Cable)", so any playback endpoint of the cable counts, preferring
/// "CABLE Input", then a stereo one.
pub fn find_cable(devices: &[Device]) -> Option<Cable> {
    let active = |flow: Flow| devices.iter().filter(move |d| d.flow == flow && d.state == DeviceState::Active);
    let capture_id = active(Flow::Capture).find(|d| d.original_name.starts_with(CABLE_CAPTURE))?.id.clone();
    let renders: Vec<&Device> = active(Flow::Render)
        .filter(|d| d.original_name.starts_with(CABLE_RENDER) || d.original_name.ends_with(CABLE_DEVICE))
        .collect();
    let render = renders
        .iter()
        .find(|d| d.original_name.starts_with(CABLE_RENDER))
        .or_else(|| renders.iter().find(|d| !d.original_name.contains("16 Ch")))
        .or(renders.first())?;
    Some(Cable { render_id: render.id.clone(), capture_id })
}

/// Engine bookkeeping that isn't saved.
#[derive(Default)]
pub struct ProcState {
    pub status: EngineStatus,
    /// The route the engine is running, if it is.
    pub route: Option<Route>,
    /// True once this run has pointed the Windows default mics at CABLE Output.
    pub switched: bool,
    pub retry_in: u32,
    pub last_error: Option<String>,
}

fn is_active_capture(devices: &[Device], id: &str) -> bool {
    devices.iter().any(|d| d.id == id && d.flow == Flow::Capture && d.state == DeviceState::Active)
}

fn default_id(devices: &[Device], flow: Flow, role: Role) -> Option<String> {
    devices
        .iter()
        .find(|d| {
            d.flow == flow
                && d.state == DeviceState::Active
                && match role {
                    Role::Console => d.default_console,
                    Role::Communications => d.default_comms,
                }
        })
        .map(|d| d.id.clone())
}

impl<B: AudioBackend> AppCore<B> {
    pub fn set_engine(&mut self, engine: Box<dyn AudioEngine>) {
        self.stop_engine();
        self.engine = engine;
    }

    pub(super) fn processing_view(&self) -> ProcessingView {
        ProcessingView {
            settings: self.settings.processing.clone(),
            status: self.engine_state.status.clone(),
            cable_installed: find_cable(&self.state.devices).is_some(),
        }
    }

    pub(super) fn engine_params(&self) -> EngineParams {
        EngineParams { dsp: self.settings.processing.dsp, monitor_level: self.settings.processing.monitor_level }
    }

    /// The mic to process: the chosen one, else the default from before processing, else the current
    /// default, else any mic. Never CABLE Output itself.
    fn source(&self, cable: &Cable) -> Option<String> {
        let devices = &self.state.devices;
        let usable = |id: Option<String>| id.filter(|id| *id != cable.capture_id && is_active_capture(devices, id));
        let remembered = self.settings.processing_restore.as_ref().and_then(|r| r.console.clone());
        usable(self.settings.processing.source_id.clone())
            .or_else(|| usable(remembered))
            .or_else(|| usable(default_id(devices, Flow::Capture, Role::Console)))
            .or_else(|| {
                devices
                    .iter()
                    .find(|d| d.flow == Flow::Capture && d.state == DeviceState::Active && d.id != cable.capture_id)
                    .map(|d| d.id.clone())
            })
    }

    fn stop_engine(&mut self) {
        if self.engine_state.route.take().is_some() {
            self.engine.stop();
        }
    }

    /// Points both default-mic roles at CABLE Output, first remembering the current ones (unless a run
    /// that didn't shut down cleanly already did). Returns true if Windows defaults changed.
    fn switch_defaults(&mut self, cable: &Cable) -> bool {
        if self.settings.processing_restore.is_none() {
            let devices = &self.state.devices;
            let current = |role| default_id(devices, Flow::Capture, role).filter(|id| *id != cable.capture_id);
            self.settings.processing_restore =
                Some(RestoreDefaults { console: current(Role::Console), comms: current(Role::Communications) });
            self.settings_dirty = true;
        }
        let mut changed = false;
        for role in [Role::Console, Role::Communications] {
            if default_id(&self.state.devices, Flow::Capture, role).as_deref() == Some(cable.capture_id.as_str()) {
                continue;
            }
            match self.backend.set_default_device(&cable.capture_id, role) {
                Ok(()) => changed = true,
                Err(e) => self.notices.push(AudioError::Os(format!("Couldn't make CABLE Output the default microphone: {e}"))),
            }
        }
        changed
    }

    /// Puts back the remembered default mics, but only for roles still pointing at CABLE Output.
    /// Returns true if Windows defaults changed.
    fn restore_defaults(&mut self) -> bool {
        self.engine_state.switched = false;
        let Some(saved) = self.settings.processing_restore.take() else { return false };
        self.settings_dirty = true;
        let Some(cable) = find_cable(&self.state.devices) else { return false };
        let mut changed = false;
        for (role, id) in [(Role::Console, saved.console), (Role::Communications, saved.comms)] {
            let Some(id) = id else { continue };
            let on_cable =
                default_id(&self.state.devices, Flow::Capture, role).as_deref() == Some(cable.capture_id.as_str());
            if on_cable && is_active_capture(&self.state.devices, &id) {
                match self.backend.set_default_device(&id, role) {
                    Ok(()) => changed = true,
                    Err(e) => tracing::warn!("couldn't restore the default mic: {e}"),
                }
            }
        }
        changed
    }

    /// Brings the engine and the Windows default mics in line with the processing settings.
    /// Returns true if it changed Windows defaults (the state must be re-read).
    pub(super) fn reconcile_processing(&mut self) -> bool {
        if !self.settings.processing.enabled {
            self.stop_engine();
            self.engine_state.status = EngineStatus::Off;
            return self.restore_defaults();
        }
        let Some(cable) = find_cable(&self.state.devices) else {
            self.stop_engine();
            self.engine_state.status = EngineStatus::NoCable;
            return false;
        };
        let mut changed = false;
        if !self.engine_state.switched {
            changed = self.switch_defaults(&cable);
            self.engine_state.switched = true;
        }
        let Some(source_id) = self.source(&cable) else {
            self.stop_engine();
            self.engine_state.status = EngineStatus::Paused { reason: "There's no microphone to process".into() };
            return changed;
        };
        let monitor_id = if self.settings.processing.monitor {
            default_id(&self.state.devices, Flow::Render, Role::Console).filter(|id| *id != cable.render_id)
        } else {
            None
        };
        let route = Route { source_id, cable_id: cable.render_id, monitor_id };

        if let Some(reason) = self.engine.failure() {
            if self.engine_state.route.take().is_some() {
                self.engine.stop();
                tracing::warn!("audio engine stopped: {reason}");
                self.engine_state.last_error = Some(reason);
                self.engine_state.retry_in = RETRY_REFRESHES;
            }
        }
        if self.engine_state.route.as_ref() == Some(&route) {
            self.engine_state.status = EngineStatus::Running;
            return changed;
        }
        self.stop_engine();
        if self.engine_state.retry_in > 0 {
            self.engine_state.retry_in -= 1;
        } else {
            match self.engine.start(&route, &self.engine_params()) {
                Ok(()) => {
                    self.engine_state.route = Some(route);
                    self.engine_state.last_error = None;
                    self.engine_state.status = EngineStatus::Running;
                    return changed;
                }
                Err(e) => {
                    tracing::warn!("audio engine didn't start: {e}");
                    self.engine_state.last_error = Some(e.to_string());
                    self.engine_state.retry_in = RETRY_REFRESHES;
                }
            }
        }
        let reason = self.engine_state.last_error.clone().unwrap_or_else(|| "Waiting for the audio devices".into());
        self.engine_state.status = EngineStatus::Paused { reason };
        changed
    }

    /// Called when Resonance quits: stops the engine and gives Windows its previous default mics back.
    /// Processing stays enabled for next time.
    pub fn shutdown(&mut self) {
        self.stop_engine();
        self.restore_defaults();
    }
}
