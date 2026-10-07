#![allow(non_snake_case)]
use windows::core::{IUnknown, IUnknown_Vtbl, GUID, HRESULT, PCWSTR};
use windows::Win32::{
    Media::Audio::ERole,
    System::Com::{CoCreateInstance, CLSCTX_ALL},
};

use super::util::to_wide;
use crate::error::Result;

const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Undocumented, stable since Windows 7. Only SetDefaultEndpoint is called; the others are
/// placeholders that keep the vtable layout correct.
#[windows_core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self) -> HRESULT;
    fn GetDeviceFormat(&self) -> HRESULT;
    fn ResetDeviceFormat(&self) -> HRESULT;
    fn SetDeviceFormat(&self) -> HRESULT;
    fn GetProcessingPeriod(&self) -> HRESULT;
    fn SetProcessingPeriod(&self) -> HRESULT;
    fn GetShareMode(&self) -> HRESULT;
    fn SetShareMode(&self) -> HRESULT;
    fn GetPropertyValue(&self) -> HRESULT;
    fn SetPropertyValue(&self) -> HRESULT;
    fn SetDefaultEndpoint(&self, device_id: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self) -> HRESULT;
}

pub fn set_default_endpoint(device_id: &str, role: ERole) -> Result<()> {
    unsafe {
        let pc: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)?;
        let wide = to_wide(device_id);
        pc.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role).ok()?;
    }
    Ok(())
}
