use windows::Win32::{
    Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
    Media::Audio::{Endpoints::IAudioEndpointVolume, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator},
    System::Com::{CoCreateInstance, StructuredStorage::PropVariantToStringAlloc, CLSCTX_ALL, STGM_READ},
};

use windows::core::HSTRING;

use super::util::take_pwstr;
use crate::error::{AudioError, Result};

pub fn create_enumerator() -> Result<IMMDeviceEnumerator> {
    unsafe { Ok(CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?) }
}

pub fn device_id(d: &IMMDevice) -> Result<String> {
    unsafe { Ok(take_pwstr(d.GetId()?)) }
}

pub fn friendly_name(d: &IMMDevice) -> Result<String> {
    unsafe {
        let store = d.OpenPropertyStore(STGM_READ)?;
        let value = store.GetValue(&PKEY_Device_FriendlyName)?;
        Ok(take_pwstr(PropVariantToStringAlloc(&value)?))
    }
}

pub fn default_id(e: &IMMDeviceEnumerator, flow: EDataFlow, role: ERole) -> Option<String> {
    unsafe { e.GetDefaultAudioEndpoint(flow, role).ok().and_then(|d| device_id(&d).ok()) }
}

pub fn endpoint_volume(d: &IMMDevice) -> Result<IAudioEndpointVolume> {
    unsafe { Ok(d.Activate(CLSCTX_ALL, None)?) }
}

pub fn device_by_id(e: &IMMDeviceEnumerator, id: &str) -> Result<IMMDevice> {
    unsafe { e.GetDevice(&HSTRING::from(id)).map_err(|_| AudioError::device_gone()) }
}
