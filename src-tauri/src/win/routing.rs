#![allow(non_snake_case)]
use std::ffi::c_void;

use windows::core::{IUnknown, IUnknown_Vtbl, HRESULT, HSTRING};
use windows::Win32::{
    Media::Audio::{eConsole, eMultimedia, eRender, EDataFlow, ERole},
    System::WinRT::RoGetActivationFactory,
};

use crate::error::Result;

const MMDEVAPI_TOKEN: &str = r"\\?\SWD#MMDEVAPI#";
const DEVINTERFACE_AUDIO_RENDER: &str = "#{e6327cad-dcec-4949-ae8a-991e976a79d2}";

pub fn pack_device_id(id: &str) -> String {
    format!("{MMDEVAPI_TOKEN}{id}{DEVINTERFACE_AUDIO_RENDER}")
}

pub fn unpack_device_id(s: &str) -> Option<String> {
    let inner = s.strip_prefix(MMDEVAPI_TOKEN)?.strip_suffix(DEVINTERFACE_AUDIO_RENDER)?;
    (!inner.is_empty()).then(|| inner.to_string())
}

/// Undocumented `IAudioPolicyConfigFactory`, Windows 10 21H2+ layout (matches EarTrumpet's
/// IAudioPolicyConfigFactoryVariantFor21H2). Declared on IUnknown with the three IInspectable
/// slots as placeholders, then 19 unused methods, then the three we call.
#[windows_core::interface("ab3d4648-e242-459f-b02f-541c70306324")]
unsafe trait IAudioPolicyConfigFactory: IUnknown {
    fn GetIids(&self) -> HRESULT;
    fn GetRuntimeClassName(&self) -> HRESULT;
    fn GetTrustLevel(&self) -> HRESULT;
    fn add_CtxVolumeChange(&self) -> HRESULT;
    fn remove_CtxVolumeChanged(&self) -> HRESULT;
    fn add_RingerVibrateStateChanged(&self) -> HRESULT;
    fn remove_RingerVibrateStateChange(&self) -> HRESULT;
    fn SetVolumeGroupGainForId(&self) -> HRESULT;
    fn GetVolumeGroupGainForId(&self) -> HRESULT;
    fn GetActiveVolumeGroupForEndpointId(&self) -> HRESULT;
    fn GetVolumeGroupsForEndpoint(&self) -> HRESULT;
    fn GetCurrentVolumeContext(&self) -> HRESULT;
    fn SetVolumeGroupMuteForId(&self) -> HRESULT;
    fn GetVolumeGroupMuteForId(&self) -> HRESULT;
    fn SetRingerVibrateState(&self) -> HRESULT;
    fn GetRingerVibrateState(&self) -> HRESULT;
    fn SetPreferredChatApplication(&self) -> HRESULT;
    fn ResetPreferredChatApplication(&self) -> HRESULT;
    fn GetPreferredChatApplication(&self) -> HRESULT;
    fn GetCurrentChatApplications(&self) -> HRESULT;
    fn add_ChatContextChanged(&self) -> HRESULT;
    fn remove_ChatContextChanged(&self) -> HRESULT;
    fn SetPersistedDefaultAudioEndpoint(&self, pid: u32, flow: EDataFlow, role: ERole, device_id: *mut c_void) -> HRESULT;
    fn GetPersistedDefaultAudioEndpoint(&self, pid: u32, flow: EDataFlow, role: ERole, device_id: *mut *mut c_void) -> HRESULT;
    fn ClearAllPersistedApplicationDefaultEndpoints(&self) -> HRESULT;
}

pub struct Routing {
    factory: IAudioPolicyConfigFactory,
}

impl Routing {
    pub fn new() -> Result<Self> {
        let class = HSTRING::from("Windows.Media.Internal.AudioPolicyConfig");
        let factory: IAudioPolicyConfigFactory = unsafe { RoGetActivationFactory(&class)? };
        Ok(Self { factory })
    }

    /// The output Windows has persisted for this process, or `None` for "default".
    pub fn get(&self, pid: u32) -> Result<Option<String>> {
        let mut raw: *mut c_void = std::ptr::null_mut();
        unsafe { self.factory.GetPersistedDefaultAudioEndpoint(pid, eRender, eMultimedia, &mut raw).ok()? };
        // Take ownership of the returned HSTRING so it is freed on drop.
        let h: HSTRING = unsafe { std::mem::transmute::<*mut c_void, HSTRING>(raw) };
        Ok(unpack_device_id(&h.to_string()))
    }

    pub fn set(&self, pid: u32, device_id: Option<&str>) -> Result<()> {
        let packed = device_id.map(|id| HSTRING::from(pack_device_id(id)));
        let raw: *mut c_void = match &packed {
            Some(h) => unsafe { std::mem::transmute_copy::<HSTRING, *mut c_void>(h) },
            None => std::ptr::null_mut(),
        };
        for role in [eMultimedia, eConsole] {
            unsafe { self.factory.SetPersistedDefaultAudioEndpoint(pid, eRender, role, raw).ok()? };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_unpacks_render_device_ids() {
        let id = "{0.0.0.00000000}.{8b1a2c3d-0000-1111-2222-333344445555}";
        let packed = pack_device_id(id);
        assert_eq!(
            packed,
            r"\\?\SWD#MMDEVAPI#{0.0.0.00000000}.{8b1a2c3d-0000-1111-2222-333344445555}#{e6327cad-dcec-4949-ae8a-991e976a79d2}"
        );
        assert_eq!(unpack_device_id(&packed).as_deref(), Some(id));
    }

    #[test]
    fn unpack_rejects_empty_and_foreign_strings() {
        assert_eq!(unpack_device_id(""), None);
        assert_eq!(unpack_device_id("garbage"), None);
        assert_eq!(unpack_device_id(r"\\?\SWD#MMDEVAPI##{e6327cad-dcec-4949-ae8a-991e976a79d2}"), None);
    }
}
