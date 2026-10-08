use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use windows::core::{implement, Result as WinResult, PCWSTR};
use windows::Win32::{
    Foundation::PROPERTYKEY,
    Media::Audio::{EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, DEVICE_STATE},
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
