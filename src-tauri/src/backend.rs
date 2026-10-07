use crate::{
    error::Result,
    model::{Peaks, Role, Snapshot},
};

/// Everything Resonance needs from the OS audio stack. Implemented by `win::WasapiBackend`
/// and by `fake::FakeBackend` in tests. Not `Send`: lives on the audio worker thread.
pub trait AudioBackend {
    fn snapshot(&mut self) -> Result<Snapshot>;
    fn set_session_volume(&mut self, session_id: &str, volume: f32) -> Result<()>;
    fn set_session_mute(&mut self, session_id: &str, muted: bool) -> Result<()>;
    fn set_device_volume(&mut self, device_id: &str, volume: f32) -> Result<()>;
    fn set_device_mute(&mut self, device_id: &str, muted: bool) -> Result<()>;
    fn set_default_device(&mut self, device_id: &str, role: Role) -> Result<()>;
    fn routing_available(&self) -> bool;
    /// `None` = follow the system default.
    fn set_app_output(&mut self, pid: u32, device_id: Option<&str>) -> Result<()>;
    fn set_mic_boost(&mut self, device_id: &str, db: f32) -> Result<()>;
    /// Peaks for sessions from the last snapshot, the default output, and `mic_device_id`
    /// (opening a capture stream for it if needed).
    fn peaks(&mut self, mic_device_id: Option<&str>) -> Peaks;
    /// Releases any capture stream opened for metering.
    fn stop_metering(&mut self);
}
