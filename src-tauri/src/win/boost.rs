use windows::Win32::Media::Audio::{IAudioVolumeLevel, IMMDevice};

use crate::{error::Result, model::Boost};

pub fn find(_dev: &IMMDevice) -> Result<Option<IAudioVolumeLevel>> {
    Ok(None)
}
pub fn read(_level: &IAudioVolumeLevel) -> Result<Boost> {
    Ok(Boost { db: 0.0, min: 0.0, max: 0.0, step: 0.0 })
}
pub fn write(_level: &IAudioVolumeLevel, _db: f32) -> Result<()> {
    Ok(())
}
