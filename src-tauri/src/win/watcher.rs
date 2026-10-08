use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use windows::core::{implement, Interface, Ref, Result as WinResult, BOOL, GUID, PCWSTR};
use windows::Win32::{
    Foundation::PROPERTYKEY,
    Media::Audio::{
        AudioSessionDisconnectReason, AudioSessionState,
        Endpoints::{IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl},
        EDataFlow, ERole, IAudioSessionControl, IAudioSessionEvents, IAudioSessionEvents_Impl, IAudioSessionNotification,
        IAudioSessionNotification_Impl, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
        AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
    },
};

/// Raised by Windows on any device add/remove/state, default-device or property (e.g. name) change.
#[implement(IMMNotificationClient)]
struct Watcher {
    dirty: Arc<AtomicBool>,
}

impl Watcher {
    fn mark(&self) -> WinResult<()> {
        self.dirty.store(true, Ordering::Release);
        Ok(())
    }
}

impl IMMNotificationClient_Impl for Watcher_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> WinResult<()> {
        self.mark()
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> WinResult<()> {
        self.mark()
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> WinResult<()> {
        self.mark()
    }
    fn OnDefaultDeviceChanged(&self, _flow: EDataFlow, _role: ERole, _id: &PCWSTR) -> WinResult<()> {
        self.mark()
    }
    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> WinResult<()> {
        self.mark()
    }
}

/// Keeps a notification callback registered on an enumerator; tells the backend when the device
/// topology must be re-read. Unregisters on drop.
pub struct DeviceWatch {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
    dirty: Arc<AtomicBool>,
}

impl DeviceWatch {
    pub fn register(enumerator: &IMMDeviceEnumerator) -> crate::error::Result<Self> {
        let dirty = Arc::new(AtomicBool::new(true));
        let client: IMMNotificationClient = Watcher { dirty: dirty.clone() }.into();
        unsafe { enumerator.RegisterEndpointNotificationCallback(&client)? };
        Ok(Self { enumerator: enumerator.clone(), client, dirty })
    }

    /// True (once) if anything changed since the last call.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }
}

impl Drop for DeviceWatch {
    fn drop(&mut self) {
        unsafe {
            let _ = self.enumerator.UnregisterEndpointNotificationCallback(&self.client);
        }
    }
}

/// One callback object for everything that changes audio state without changing the device list: a session
/// appearing, a session's volume, mute, state or name changing, and a device's volume or mute changing.
/// Each just raises a flag, so the backend re-reads Windows only when something actually changed.
#[implement(IAudioSessionNotification, IAudioSessionEvents, IAudioEndpointVolumeCallback)]
struct StateWatcher {
    dirty: Arc<AtomicBool>,
}

impl StateWatcher {
    fn mark(&self) -> WinResult<()> {
        self.dirty.store(true, Ordering::Release);
        Ok(())
    }
}

impl IAudioSessionNotification_Impl for StateWatcher_Impl {
    fn OnSessionCreated(&self, _session: Ref<IAudioSessionControl>) -> WinResult<()> {
        self.mark()
    }
}

impl IAudioSessionEvents_Impl for StateWatcher_Impl {
    fn OnDisplayNameChanged(&self, _name: &PCWSTR, _context: *const GUID) -> WinResult<()> {
        self.mark()
    }
    fn OnIconPathChanged(&self, _path: &PCWSTR, _context: *const GUID) -> WinResult<()> {
        Ok(())
    }
    fn OnSimpleVolumeChanged(&self, _volume: f32, _mute: BOOL, _context: *const GUID) -> WinResult<()> {
        self.mark()
    }
    fn OnChannelVolumeChanged(&self, _count: u32, _volumes: *const f32, _changed: u32, _context: *const GUID) -> WinResult<()> {
        Ok(())
    }
    fn OnGroupingParamChanged(&self, _param: *const GUID, _context: *const GUID) -> WinResult<()> {
        Ok(())
    }
    fn OnStateChanged(&self, _state: AudioSessionState) -> WinResult<()> {
        self.mark()
    }
    fn OnSessionDisconnected(&self, _reason: AudioSessionDisconnectReason) -> WinResult<()> {
        self.mark()
    }
}

impl IAudioEndpointVolumeCallback_Impl for StateWatcher_Impl {
    fn OnNotify(&self, _data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> WinResult<()> {
        self.mark()
    }
}

/// Raised when audio state (sessions, volumes, mutes) may have changed. The backend registers its
/// callbacks on each session manager, session and device volume, and unregisters them when it drops them.
pub struct StateWatch {
    dirty: Arc<AtomicBool>,
    pub sessions: IAudioSessionNotification,
    pub session_events: IAudioSessionEvents,
    pub volumes: IAudioEndpointVolumeCallback,
}

impl StateWatch {
    pub fn new() -> Self {
        let dirty = Arc::new(AtomicBool::new(true));
        let watcher: IAudioSessionNotification = StateWatcher { dirty: dirty.clone() }.into();
        // Same object behind all three interfaces.
        let session_events = watcher.cast().expect("StateWatcher implements IAudioSessionEvents");
        let volumes = watcher.cast().expect("StateWatcher implements IAudioEndpointVolumeCallback");
        Self { dirty, sessions: watcher, session_events, volumes }
    }

    /// True (once) if anything changed since the last call.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }
}

impl Default for StateWatch {
    fn default() -> Self {
        Self::new()
    }
}
