use std::collections::{HashMap, HashSet};

use windows::core::Interface;
use windows::Win32::Foundation::S_OK;
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, AudioSessionStateExpired,
    Endpoints::IAudioMeterInformation, IAudioSessionControl2, IAudioSessionManager2, IAudioVolumeLevel, IMMDevice,
    IMMDeviceEnumerator, ISimpleAudioVolume, DEVICE_STATE, DEVICE_STATE_ACTIVE, DEVICE_STATE_UNPLUGGED,
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
};
use crate::{
    backend::AudioBackend,
    error::{AudioError, Result},
    model::*,
};

struct SessionHandle {
    volume: ISimpleAudioVolume,
    meter: IAudioMeterInformation,
}

/// WASAPI implementation of `AudioBackend`. Must be created and used on one thread.
pub struct WasapiBackend {
    enumerator: IMMDeviceEnumerator,
    endpoints: HashMap<String, IMMDevice>,
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
        Ok(Self {
            enumerator: create_enumerator()?,
            endpoints: HashMap::new(),
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

    fn endpoint(&self, id: &str) -> Result<&IMMDevice> {
        self.endpoints.get(id).ok_or_else(|| AudioError::NotFound("That device is no longer connected".into()))
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
        let dev = self.endpoint(id)?.clone();
        let level = boost::find(&dev).unwrap_or_else(|e| {
            tracing::debug!("boost lookup failed for {id}: {e}");
            None
        });
        self.boosts.insert(id.to_string(), level.clone());
        Ok(level)
    }

    fn canonical_device(&self, id: &str) -> Option<String> {
        self.endpoints.keys().find(|k| k.eq_ignore_ascii_case(id)).cloned()
    }

    fn snapshot_inner(&mut self) -> Result<Snapshot> {
        let e = &self.enumerator;
        let defaults = [
            default_id(e, eRender, eConsole),
            default_id(e, eRender, eCommunications),
            default_id(e, eCapture, eConsole),
            default_id(e, eCapture, eCommunications),
        ];

        // Pass 1: enumerate endpoints.
        let mut found: Vec<(String, IMMDevice, Flow, DeviceState)> = Vec::new();
        for (flow, edf) in [(Flow::Render, eRender), (Flow::Capture, eCapture)] {
            unsafe {
                let coll = self.enumerator.EnumAudioEndpoints(edf, DEVICE_STATE(DEVICE_STATE_ACTIVE.0 | DEVICE_STATE_UNPLUGGED.0))?;
                for i in 0..coll.GetCount()? {
                    let dev = coll.Item(i)?;
                    let id = device_id(&dev)?;
                    let state =
                        if dev.GetState()? == DEVICE_STATE_ACTIVE { DeviceState::Active } else { DeviceState::Unplugged };
                    found.push((id, dev, flow, state));
                }
            }
        }
        self.endpoints = found.iter().map(|(id, dev, _, _)| (id.clone(), dev.clone())).collect();
        let active_capture: HashSet<String> = found
            .iter()
            .filter(|(_, _, f, s)| *f == Flow::Capture && *s == DeviceState::Active)
            .map(|(id, ..)| id.clone())
            .collect();
        self.boosts.retain(|id, _| active_capture.contains(id));

        // Pass 2: device properties.
        let mut devices = Vec::with_capacity(found.len());
        for (id, dev, flow, state) in &found {
            let (volume, muted) = if *state == DeviceState::Active {
                endpoint_volume(dev)
                    .and_then(|v| unsafe { Ok((v.GetMasterVolumeLevelScalar()?, v.GetMute()?.as_bool())) })
                    .unwrap_or((0.0, false))
            } else {
                (0.0, false)
            };
            let boost = if active_capture.contains(id) {
                self.boost_level(id).ok().flatten().and_then(|l| boost::read(&l).ok())
            } else {
                None
            };
            let (dc, dm) = match flow {
                Flow::Render => (&defaults[0], &defaults[1]),
                Flow::Capture => (&defaults[2], &defaults[3]),
            };
            devices.push(RawDevice {
                id: id.clone(),
                name: friendly_name(dev).unwrap_or_else(|_| "Unknown device".into()),
                flow: *flow,
                state: *state,
                default_console: dc.as_deref() == Some(id.as_str()),
                default_comms: dm.as_deref() == Some(id.as_str()),
                volume,
                muted,
                boost,
            });
        }

        // Pass 3: sessions on every active output.
        self.sessions.clear();
        let mut sessions = Vec::new();
        let mut live_pids = HashSet::new();
        let outputs: Vec<(String, IMMDevice)> = found
            .iter()
            .filter(|(_, _, f, s)| *f == Flow::Render && *s == DeviceState::Active)
            .map(|(id, dev, ..)| (id.clone(), dev.clone()))
            .collect();
        for (dev_id, dev) in outputs {
            let Ok(manager) = (unsafe { dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) }) else { continue };
            unsafe {
                let list = manager.GetSessionEnumerator()?;
                for i in 0..list.GetCount()? {
                    let Ok(ctl) = list.GetSession(i) else { continue };
                    if ctl.GetState().map(|s| s == AudioSessionStateExpired).unwrap_or(true) {
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
                    let (Ok(volume), Ok(meter)) = (ctl.cast::<ISimpleAudioVolume>(), ctl.cast::<IAudioMeterInformation>())
                    else {
                        continue;
                    };
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
                    self.sessions.insert(id, SessionHandle { volume, meter });
                }
            }
        }
        self.procs.retain(|pid, _| live_pids.contains(pid));

        self.master_meter = defaults[0]
            .as_ref()
            .and_then(|id| self.endpoints.get(id))
            .and_then(|d| unsafe { d.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None) }.ok());

        Ok(Snapshot { devices, sessions })
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
                self.boosts.clear();
                self.mic_meter = None;
                self.snapshot_inner()
            }
        }
    }

    fn set_session_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        let h = self.sessions.get(id).ok_or_else(|| AudioError::NotFound("That audio session has ended".into()))?;
        unsafe { h.volume.SetMasterVolume(volume, std::ptr::null())? };
        Ok(())
    }

    fn set_session_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        let h = self.sessions.get(id).ok_or_else(|| AudioError::NotFound("That audio session has ended".into()))?;
        unsafe { h.volume.SetMute(muted, std::ptr::null())? };
        Ok(())
    }

    fn set_device_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        let v = endpoint_volume(self.endpoint(id)?)?;
        unsafe { v.SetMasterVolumeLevelScalar(volume, std::ptr::null())? };
        Ok(())
    }

    fn set_device_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        let v = endpoint_volume(self.endpoint(id)?)?;
        unsafe { v.SetMute(muted, std::ptr::null())? };
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
        Ok(())
    }

    fn routing_available(&self) -> bool {
        self.routing.is_some()
    }

    fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> Result<()> {
        self.routing
            .as_ref()
            .ok_or_else(|| AudioError::Unavailable("Per-app routing isn't available".into()))?
            .set(pid, device_id)
    }

    fn set_mic_boost(&mut self, id: &str, db: f32) -> Result<()> {
        let level =
            self.boost_level(id)?.ok_or_else(|| AudioError::Unavailable("This microphone has no boost control".into()))?;
        boost::write(&level, db)
    }

    fn peaks(&mut self, mic_device_id: Option<&str>) -> Peaks {
        let sessions =
            self.sessions.iter().map(|(id, h)| (id.clone(), unsafe { h.meter.GetPeakValue() }.unwrap_or(0.0))).collect();
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
                    match self.endpoint(id).and_then(|d| MicMeter::open(d, id)) {
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
}
