use crate::{
    backend::AudioBackend,
    error::{AudioError, Result},
    model::*,
};

#[derive(Default)]
pub struct FakeBackend {
    pub devices: Vec<RawDevice>,
    pub sessions: Vec<RawSession>,
    pub routing: bool,
    pub calls: Vec<String>,
    pub snapshots: usize,
    pub peaks: Peaks,
    pub fail_next_snapshot: bool,
}

impl FakeBackend {
    pub fn device(id: &str, flow: Flow) -> RawDevice {
        RawDevice {
            id: id.into(),
            name: format!("Device {id}"),
            flow,
            state: DeviceState::Active,
            default_console: false,
            default_comms: false,
            volume: 0.5,
            muted: false,
            boost: None,
        }
    }

    pub fn session(id: &str, pid: u32, exe: Option<&str>) -> RawSession {
        RawSession {
            id: id.into(),
            pid,
            exe_path: exe.map(|e| format!("C:\\Apps\\{e}")),
            display_name: None,
            is_system: false,
            device_id: "out1".into(),
            volume: 1.0,
            muted: false,
            output: None,
        }
    }

    fn device_mut(&mut self, id: &str) -> Result<&mut RawDevice> {
        self.devices.iter_mut().find(|d| d.id == id).ok_or_else(|| AudioError::NotFound(id.into()))
    }

    fn session_mut(&mut self, id: &str) -> Result<&mut RawSession> {
        self.sessions.iter_mut().find(|s| s.id == id).ok_or_else(|| AudioError::NotFound(id.into()))
    }
}

impl AudioBackend for FakeBackend {
    fn snapshot(&mut self) -> Result<Snapshot> {
        self.snapshots += 1;
        if std::mem::take(&mut self.fail_next_snapshot) {
            return Err(AudioError::Os("fake failure".into()));
        }
        Ok(Snapshot { devices: self.devices.clone(), sessions: self.sessions.clone() })
    }

    fn set_session_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        self.calls.push(format!("session_volume {id} {volume:.2}"));
        self.session_mut(id)?.volume = volume;
        Ok(())
    }

    fn set_session_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        self.calls.push(format!("session_mute {id} {muted}"));
        self.session_mut(id)?.muted = muted;
        Ok(())
    }

    fn set_device_volume(&mut self, id: &str, volume: f32) -> Result<()> {
        self.calls.push(format!("device_volume {id} {volume:.2}"));
        self.device_mut(id)?.volume = volume;
        Ok(())
    }

    fn set_device_mute(&mut self, id: &str, muted: bool) -> Result<()> {
        self.calls.push(format!("device_mute {id} {muted}"));
        self.device_mut(id)?.muted = muted;
        Ok(())
    }

    fn set_default_device(&mut self, id: &str, role: Role) -> Result<()> {
        self.calls.push(format!("default {id} {role:?}"));
        let flow = self.device_mut(id)?.flow;
        for d in self.devices.iter_mut().filter(|d| d.flow == flow) {
            match role {
                Role::Console => d.default_console = d.id == id,
                Role::Communications => d.default_comms = d.id == id,
            }
        }
        Ok(())
    }

    fn routing_available(&self) -> bool {
        self.routing
    }

    fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> Result<()> {
        if !self.routing {
            return Err(AudioError::Unavailable("routing".into()));
        }
        self.calls.push(format!("output {pid} {device_id:?}"));
        for s in self.sessions.iter_mut().filter(|s| s.pid == pid) {
            s.output = device_id.map(String::from);
        }
        Ok(())
    }

    fn set_mic_boost(&mut self, id: &str, db: f32) -> Result<()> {
        self.calls.push(format!("boost {id} {db:.1}"));
        let d = self.device_mut(id)?;
        let b = d.boost.as_mut().ok_or_else(|| AudioError::Unavailable("no boost".into()))?;
        b.db = db;
        Ok(())
    }

    fn peaks(&mut self, mic_device_id: Option<&str>) -> Peaks {
        self.calls.push(format!("peaks {mic_device_id:?}"));
        self.peaks.clone()
    }

    fn stop_metering(&mut self) {
        self.calls.push("stop_metering".into());
    }
}
