use std::{collections::HashSet, ffi::c_void};

use windows::core::Interface;
use windows::Win32::{
    Media::Audio::{IAudioVolumeLevel, IDeviceTopology, IMMDevice, IPart},
    System::Com::CLSCTX_ALL,
};

use super::util::take_pwstr;
use crate::{error::Result, model::Boost};

/// Finds the "Microphone Boost" control upstream of a capture endpoint, if the driver exposes one.
pub fn find(dev: &IMMDevice) -> Result<Option<IAudioVolumeLevel>> {
    unsafe {
        let topo: IDeviceTopology = dev.Activate(CLSCTX_ALL, None)?;
        let connector = topo.GetConnector(0)?;
        let Ok(other) = connector.GetConnectedTo() else { return Ok(None) };
        let part: IPart = other.cast()?;
        let mut visited = HashSet::new();
        Ok(walk(&part, &mut visited, 0))
    }
}

unsafe fn walk(part: &IPart, visited: &mut HashSet<u32>, depth: u32) -> Option<IAudioVolumeLevel> {
    if depth > 16 {
        return None;
    }
    let id = part.GetLocalId().ok()?;
    if !visited.insert(id) {
        return None;
    }
    if let Ok(name) = part.GetName() {
        if take_pwstr(name).to_lowercase().contains("boost") {
            let mut raw: *mut c_void = std::ptr::null_mut();
            if part.Activate(CLSCTX_ALL.0, &IAudioVolumeLevel::IID, Some(&mut raw)).is_ok() && !raw.is_null() {
                return Some(IAudioVolumeLevel::from_raw(raw));
            }
        }
    }
    let parts = part.EnumPartsIncoming().ok()?;
    for i in 0..parts.GetCount().ok()? {
        if let Ok(p) = parts.GetPart(i) {
            if let Some(level) = walk(&p, visited, depth + 1) {
                return Some(level);
            }
        }
    }
    None
}

pub fn read(level: &IAudioVolumeLevel) -> Result<Boost> {
    let (mut min, mut max, mut step) = (0f32, 0f32, 0f32);
    unsafe {
        level.GetLevelRange(0, &mut min, &mut max, &mut step)?;
        let db = level.GetLevel(0)?;
        Ok(Boost { db, min, max, step })
    }
}

pub fn write(level: &IAudioVolumeLevel, db: f32) -> Result<()> {
    unsafe { level.SetLevelUniform(db, None)? };
    Ok(())
}
