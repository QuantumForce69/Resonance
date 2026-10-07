use crate::error::{AudioError, Result};

pub struct Routing;

impl Routing {
    pub fn new() -> Result<Self> {
        Err(AudioError::Unavailable("not implemented yet".into()))
    }
    pub fn get(&self, _pid: u32) -> Result<Option<String>> {
        Ok(None)
    }
    pub fn set(&self, _pid: u32, _device_id: Option<&str>) -> Result<()> {
        Ok(())
    }
}
