use windows::Win32::Media::Audio::IMMDevice;

use crate::error::{AudioError, Result};

pub struct MicMeter {
    pub device_id: String,
}

impl MicMeter {
    pub fn open(_dev: &IMMDevice, _id: &str) -> Result<Self> {
        Err(AudioError::Unavailable("not implemented yet".into()))
    }
    pub fn peak(&self) -> f32 {
        0.0
    }
}
