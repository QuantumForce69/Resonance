use serde::{ser::SerializeStruct, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AudioError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Os(String),
}

impl AudioError {
    pub fn code(&self) -> &'static str {
        match self {
            AudioError::NotFound(_) => "notFound",
            AudioError::Unavailable(_) => "unavailable",
            AudioError::Invalid(_) => "invalid",
            AudioError::Os(_) => "os",
        }
    }
}

impl Serialize for AudioError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AudioError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for AudioError {
    fn from(e: windows::core::Error) -> Self {
        AudioError::Os(format!("{} (0x{:08X})", e.message(), e.code().0 as u32))
    }
}

pub type Result<T> = std::result::Result<T, AudioError>;
