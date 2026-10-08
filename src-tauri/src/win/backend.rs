use std::collections::{HashMap, HashSet};

use windows::core::Interface;
use windows::Win32::Foundation::S_OK;
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, AudioSessionStateActive, AudioSessionStateExpired,
    Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioMeterInformation},
    IAudioSessionControl, IAudioSessionControl2, IAudioSessionEvents, IAudioSessionNotification, IAudioSessionManager2, IAudioVolumeLevel, IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume,
    DEVICE_STATE, DEVICE_STATE_ACTIVE, DEVICE_STATE_UNPLUGGED,
};
use windows::Win32::System::Com::CLSCTX_ALL;

use super::{
    boost,
    com::ComGuard,
    endpoints::{create_enumerator, default_id, device_id, endpoint_volume, friendly_name},
    meter::MicMeter,
    policy,
    procinfo::{self, ProcLookup},
    routing::Routing,
    util::take_pwstr,
    watcher::{DeviceWatch, StateWatch},
};
use crate::{
    backend::AudioBackend,
    error::{AudioError, Result},
    model::*,
};

/// Safety net: re-read everything at least this often (in polls) even if Windows sends no notification
/// (per-app routing changed in Windows Settings and mic boost have none). At the 250 ms poll rate this is
/// every 10 s.
const FULL_REFRESH_POLLS: u32 = 40;

struct SessionHandle {
    volume: ISimpleAudioVolume,
    meter: IAudioMeterInformation,
    /// Registered for change notifications while this handle lives.
    control: IAudioSessionControl,
    events: IAudioSessionEvents,
    /// Playing right now (as of the last read; a state change triggers a re-read). Silent sessions' peaks
    /// are known to be zero, so meters skip asking Windows for them.
    active: bool,
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = self.control.UnregisterAudioSessionNotification(&self.events);
        }
    }
}

/// Everything about an endpoint that only changes when Windows reports a topology change.
struct Endpoint {
    id: String,
    device: IMMDevice,
    flow: Flow,
    state: DeviceState,
    name: String,
    /// Active endpoints only.
    volume: Option<IAudioEndpointVolume>,
    /// Active outputs only.
    sessions: Option<IAudioSessionManager2>,
    /// Callbacks registered on `volume` / `sessions`, unregistered on drop.
    volume_watch: Option<IAudioEndpointVolumeCallback>,
    session_watch: Option<IAudioSessionNotification>,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        unsafe {
            if let (Some(v), Some(cb)) = (&self.volume, &self.volume_watch) {
                let _ = v.UnregisterControlChangeNotify(cb);
            }
            if let (Some(m), Some(cb)) = (&self.sessions, &self.session_watch) {
                let _ = m.UnregisterSessionNotification(cb);
            }
        }
    }
}

/// WASAPI implementation of `AudioBackend`. Must be created and used on one thread.
///
/// The device list, defaults, names and per-device COM objects are cached and rebuilt only when
/// Windows notifies a device change. Volumes, mutes and sessions are re-read only when Windows notifies a
/// session or volume change (or after Resonance changes something itself); otherwise a poll returns the
/// last snapshot. Everything is re-read every `FULL_REFRESH_POLLS` as a safety net.
pub struct WasapiBackend {
    watch: DeviceWatch,
    changes: StateWatch,
    cached: Option<Snapshot>,
    enumerator: IMMDeviceEnumerator,
    endpoints: Vec<Endpoint>,
    defaults: [Option<String>; 4],
    polls_since_full: u32,
    sessions: HashMap<String, SessionHandle>,
    boosts: HashMap<String, Option<IAudioVolumeLevel>>,
    procs: HashMap<u32, ProcLookup>,
    routing: Option<Routing>,
    master_meter: Option<IAudioMeterInformation>,
    mic_meter: Option<MicMeter>,
    mic_meter_failed: Option<String>,
    // Declared last: COM is uninitialized only after every COM object above is released.
    _com: ComGuard,
}

impl WasapiBackend {
    pub fn new() -> Result<Self> {
        let com = ComGuard::init_mta()?;
        let routing = match Routing::new() {
            Ok(r) => Some(r),
            Err(e) => {
                tracing::warn!("per-app routing unavailable: {e}");
                None
            }
        };
        let enumerator = create_enumerator()?;
        Ok(Self {
            watch: DeviceWatch::register(&enumerator)?,
            changes: StateWatch::new(),
            cached: None,
            enumerator,
            endpoints: Vec::new(),
            defaults: Default::default(),
            polls_since_full: 0,
            sessions: HashMap::new(),
            boosts: HashMap::new(),
            procs: HashMap::new(),
            routing,
            master_meter: None,
            mic_meter: None,
            mic_meter_failed: None,
            _com: com,
        })
    }

    fn endpoint(&self, id: &str) -> Result<&Endpoint> {
        self.endpoints
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(AudioError::device_gone)
    }

    fn endpoint_volume(&self, id: &str) -> Result<IAudioEndpointVolume> {
        let e = self.endpoint(id)?;
        match &e.volume {
            Some(v) => Ok(v.clone()),
            None => endpoint_volume(&e.device),
        }
    }

    fn proc(&mut self, pid: u32) -> ProcLookup {
        if let Some(p) = self.procs.get(&pid) {
            return p.clone();
        }
        let p = procinfo::lookup(pid);
        if !matches!(p, ProcLookup::Dead) {
            self.procs.insert(pid, p.clone());
        }
        p
    }

    fn boost_level(&mut self, id: &str) -> Result<Option<IAudioVolumeLevel>> {
        if let Some(cached) = self.boosts.get(id) {
            return Ok(cached.clone());
        }
        let dev = self.endpoint(id)?.device.clone();
        let level = boost::find(&dev).unwrap_or_else(|e| {
            tracing::debug!("boost lookup failed for {id}: {e}");
            None
        });
        self.boosts.insert(id.to_string(), level.clone());
        Ok(level)
    }

    fn canonical_device(&self, id: &str) -> Option<String> {
        self.endpoints.iter().find(|e| e.id.eq_ignore_ascii_case(id)).map(|e| e.id.clone())
    }

    /// Re-reads defaults, the endpoint list, names and per-device COM objects.
    fn refresh_topology(&mut self) -> Result<()> {
        let e = &self.enumerator;
        self.defaults = [
            default_id(e, eRender, eConsole),
            default_id(e, eRender, eCommunications),
            default_id(e, eCapture, eConsole),
            default_id(e, eCapture, eCommunications),
        ];
        let mut endpoints = Vec::new();
        for (flow, edf) in [(Flow::Render, eRender), (Flow::Capture, eCapture)] {
            unsafe {
                let coll = self.enumerator.EnumAudioEndpoints(edf, DEVICE_STATE(DEVICE_STATE_ACTIVE.0 | DEVICE_STATE_UNPLUGGED.0))?;
                for i in 0..coll.GetCount()? {
                    let device = coll.Item(i)?;
                    let id = device_id(&device)?;
                    let active = device.GetState()? == DEVICE_STATE_ACTIVE;
                    let volume = if active { endpoint_volume(&device).ok() } else { None };
                    let sessions = if active && flow == Flow::Render {
                        device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None).ok()
                    } else {
                        None
                    };
                    let volume_watch = volume
                        .as_ref()
                        .filter(|v| v.RegisterControlChangeNotify(&self.changes.volumes).is_ok())
                        .map(|_| self.changes.volumes.clone());
                    let session_watch = sessions
                        .as_ref()
                        .filter(|m| m.RegisterSessionNotification(&self.changes.sessions).is_ok())
                        .map(|_| self.changes.sessions.clone());
                    endpoints.push(Endpoint {
                        name: friendly_name(&device).unwrap_or_else(|_| "Unknown device".into()),
                        volume,
                        sessions,
                        volume_watch,
                        session_watch,
                        state: if active { DeviceState::Active } else { DeviceState::Unplugged },
                        id,
                        device,
                        flow,
                    });
                }
            }
        }
        let active_capture: HashSet<&str> = endpoints
            .iter()
            .filter(|e| e.flow == Flow::Capture && e.state == DeviceState::Active)
            .map(|e| e.id.as_str())
            .collect();
        self.boosts.retain(|id, _| active_capture.contains(id.as_str()));
        self.master_meter = self.defaults[0]
            .as_ref()
            .and_then(|id| endpoints.iter().find(|e| &e.id == id))
            .and_then(|e| unsafe { e.device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) }.ok());
        self.endpoints = endpoints;
        self.polls_since_full = 0;
        Ok(())
    }

    fn snapshot_inner(&mut self) -> Result<Snapshot> {
        let topology_changed =
            self.watch.take_dirty() || self.polls_since_full >= FULL_REFRESH_POLLS || self.endpoints.is_empty();
        let state_changed = self.changes.take_dirty();
        if !topology_changed && !state_changed {
            if let Some(cached) = &self.cached {
                self.polls_since_full += 1;
                return Ok(cached.clone());
            }
        }
        if topology_changed {
            self.refresh_topology()?;
        } else {
            self.polls_since_full += 1;
        }

        // Devices: cached identity, live volume/mute/boost.
        let mut devices = Vec::with_capacity(self.endpoints.len());
        for i in 0..self.endpoints.len() {
            let (volume, muted) = match &self.endpoints[i].volume {
                Some(v) => match unsafe { (v.GetMasterVolumeLevelScalar(), v.GetMute()) } {
                    (Ok(level), Ok(mute)) => (level, mute.as_bool()),
                    _ => {
                        // The cached object went stale; rebuild the topology next poll.
                        self.watch.mark_dirty();
                        (0.0, false)
                    }
                },
                None => (0.0, false),
            };
            let ep = &self.endpoints[i];
            let (id, flow, state, name) = (ep.id.clone(), ep.flow, ep.state, ep.name.clone());
            let boost = if flow == Flow::Capture && state == DeviceState::Active {
                self.boost_level(&id).ok().flatten().and_then(|l| boost::read(&l).ok())
            } else {
                None
            };
            let (dc, dm) = match flow {
                Flow::Render => (&self.defaults[0], &self.defaults[1]),
                Flow::Capture => (&self.defaults[2], &self.defaults[3]),
            };
            devices.push(RawDevice {
                default_console: dc.as_deref() == Some(id.as_str()),
                default_comms: dm.as_deref() == Some(id.as_str()),
                id,
                name,
                flow,
                state,
                volume,
                muted,
                boost,
            });
        }

        // Sessions on every active output. Handles of sessions seen before are kept (with their
        // notifications); new ones are registered; ones that went away are dropped (unregistered).
        let mut previous = std::mem::take(&mut self.sessions);
        let mut sessions = Vec::new();
        let mut live_pids = HashSet::new();
        let managers: Vec<(String, IAudioSessionManager2)> =
            self.endpoints.iter().filter_map(|e| Some((e.id.clone(), e.sessions.clone()?))).collect();
        for (dev_id, manager) in managers {
            unsafe {
                let list = manager.GetSessionEnumerator()?;
                for i in 0..list.GetCount()? {
                    let Ok(ctl) = list.GetSession(i) else { continue };
                    let Ok(state) = ctl.GetState() else { continue };
                    if state == AudioSessionStateExpired {
                        continue;
                    }
                    let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                    let is_system = ctl2.IsSystemSoundsSession() == S_OK;
                    let pid = ctl2.GetProcessId().unwrap_or(0);
                    let (exe_path, description) = if is_system || pid == 0 {
                        (None, None)
                    } else {
                        match self.proc(pid) {
                            ProcLookup::Dead => continue,
                            ProcLookup::Unknown => (None, None),
                            ProcLookup::Found(p) => (Some(p.exe_path), p.description),
                        }
                    };
                    live_pids.insert(pid);
                    let Ok(id) = ctl2.GetSessionInstanceIdentifier().map(|p| take_pwstr(p)) else { continue };
                    let mut handle = match previous.remove(&id) {
                        Some(h) => h,
                        None => {
                            let (Ok(volume), Ok(meter)) =
                                (ctl.cast::<ISimpleAudioVolume>(), ctl.cast::<IAudioMeterInformation>())
                            else {
                                continue;
                            };
                            let events = self.changes.session_events.clone();
                            let _ = ctl.RegisterAudioSessionNotification(&events);
                            SessionHandle { volume, meter, control: ctl.clone(), events, active: false }
                        }
                    };
                    handle.active = state == AudioSessionStateActive;
                    let volume = &handle.volume;
                    let level = volume.GetMasterVolume().unwrap_or(1.0);
                    let muted = volume.GetMute().map(|m| m.as_bool()).unwrap_or(false);
                    let session_name = ctl
                        .GetDisplayName()
                        .ok()
                        .map(|p| take_pwstr(p))
                        .filter(|s| !s.is_empty() && !s.starts_with('@'));
                    let output = if is_system {
                        None
                    } else {
                        self.routing
                            .as_ref()
                            .and_then(|r| r.get(pid).ok().flatten())
                            .map(|o| self.canonical_device(&o).unwrap_or(o))
                    };
                    sessions.push(RawSession {
                        id: id.clone(),
                        pid,
                        exe_path,
                        display_name: session_name.or(description),
                        is_system,
                        device_id: dev_id.clone(),
                        volume: level,
                        muted,
                        output,
                    });
                    self.sessions.insert(id, handle);
                }
            }
        }
        self.procs.retain(|pid, _| live_pids.contains(pid));

        let snapshot = Snapshot { devices, sessions };
        self.cached = Some(snapshot.clone());
        Ok(snapshot)
    }
}

impl AudioBackend for WasapiBackend {
    fn snapshot(&mut self) -> Result<Snapshot> {
        match self.snapshot_inner() {
            Ok(s) => Ok(s),
            Err(e) => {
                // Audio-service restart / sleep-resume invalidates the enumerator: rebuild once.
                tracing::warn!("snapshot failed ({e}); recreating device enumerator");
                self.enumerator = create_enumerator()?;
                self.watch = DeviceWatch::register(&self.enumerator)?;
                self.endpoints.clear();
                self.boosts.clear();
                self.mic_meter = None;
                self.snapshot_inner()
            }
        }
    }

    fn set_session_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        let h = self.sessions.get(id).ok_or_else(AudioError::session_ended)?;
        unsafe { h.volume.SetMasterVolume(volume, std::ptr::null())? };
        self.changes.mark_dirty();
        Ok(())
    }

    fn set_session_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        let h = self.sessions.get(id).ok_or_else(AudioError::session_ended)?;
        unsafe { h.volume.SetMute(muted, std::ptr::null())? };
        self.changes.mark_dirty();
        Ok(())
    }

    fn set_device_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        let v = self.endpoint_volume(id)?;
        unsafe { v.SetMasterVolumeLevelScalar(volume, std::ptr::null())? };
        self.changes.mark_dirty();
        Ok(())
    }

    fn set_device_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        let v = self.endpoint_volume(id)?;
        unsafe { v.SetMute(muted, std::ptr::null())? };
        self.changes.mark_dirty();
        Ok(())
    }

    fn set_default_device(&mut self, id: &str, role: Role) -> Result<()> {
        self.endpoint(id)?;
        let roles = match role {
            Role::Console => vec![eConsole, eMultimedia],
            Role::Communications => vec![eCommunications],
        };
        for r in roles {
            policy::set_default_endpoint(id, r)?;
        }
        // Windows' notification is asynchronous; re-read defaults on the refresh that follows.
        self.watch.mark_dirty();
        Ok(())
    }

    fn routing_available(&self) -> bool {
        self.routing.is_some()
    }

    fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> Result<()> {
        self.routing
            .as_ref()
            .ok_or_else(|| AudioError::Unavailable("Per-app routing isn't available".into()))?
            .set(pid, device_id)?;
        self.changes.mark_dirty();
        Ok(())
    }

    fn set_mic_boost(&mut self, id: &str, db: f32) -> Result<()> {
        let level =
            self.boost_level(id)?.ok_or_else(AudioError::no_boost)?;
        boost::write(&level, db)?;
        self.changes.mark_dirty();
        Ok(())
    }

    fn peaks(&mut self, mic_device_id: Option<&str>) -> Peaks {
        let sessions =
            self.sessions
                .iter()
                .map(|(id, h)| (id.clone(), if h.active { unsafe { h.meter.GetPeakValue() }.unwrap_or(0.0) } else { 0.0 }))
                .collect();
        let master = self.master_meter.as_ref().and_then(|m| unsafe { m.GetPeakValue() }.ok()).unwrap_or(0.0);
        let mic = match mic_device_id {
            None => {
                self.mic_meter = None;
                0.0
            }
            Some(id) => {
                let current = self.mic_meter.as_ref().map(|m| m.device_id == id).unwrap_or(false);
                let failed_before = self.mic_meter_failed.as_deref() == Some(id);
                if !current && !failed_before {
                    self.mic_meter = None;
                    match self.endpoint(id).and_then(|e| MicMeter::open(&e.device, id)) {
                        Ok(m) => {
                            self.mic_meter = Some(m);
                            self.mic_meter_failed = None;
                        }
                        Err(e) => {
                            tracing::warn!("couldn't open mic meter: {e}");
                            self.mic_meter_failed = Some(id.to_string());
                        }
                    }
                }
                self.mic_meter.as_ref().map(|m| m.peak()).unwrap_or(0.0)
            }
        };
        Peaks { sessions, master, mic }
    }

    fn stop_metering(&mut self) {
        self.mic_meter = None;
        self.mic_meter_failed = None;
    }

    fn process_parents(&mut self) -> HashMap<u32, u32> {
        super::procinfo::parent_pids()
    }
}
