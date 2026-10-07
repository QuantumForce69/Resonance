use std::sync::Arc;

use async_trait::async_trait;
use openscq30_lib::{
    connection::ConnectionStatus,
    device::OpenSCQ30Device,
    settings::{Setting, SettingId, Value},
};
use tokio::sync::watch;

use super::mapping::SettingSource;

/// The subset of a connected headset Resonance uses. Implemented by `LibDevice` and test fakes.
#[async_trait]
pub trait HeadsetDevice: SettingSource + Send + Sync {
    fn watch_for_changes(&self) -> watch::Receiver<()>;
    fn connection_status(&self) -> watch::Receiver<ConnectionStatus>;
    async fn set_setting_values(&self, values: Vec<(SettingId, Value)>) -> Result<(), String>;
}

pub struct Found {
    pub name: String,
    pub device: Arc<dyn HeadsetDevice>,
}

/// Finds and connects to the Space One. `Ok(None)` means Windows knows of no Space One.
#[async_trait]
pub trait HeadsetConnector: Send + Sync {
    async fn connect(&self) -> Result<Option<Found>, String>;
}

pub struct LibDevice(pub Arc<dyn OpenSCQ30Device + Send + Sync>);

impl SettingSource for LibDevice {
    fn setting(&self, id: &SettingId) -> Option<Setting> {
        self.0.setting(id)
    }
}

#[async_trait]
impl HeadsetDevice for LibDevice {
    fn watch_for_changes(&self) -> watch::Receiver<()> {
        self.0.watch_for_changes()
    }

    fn connection_status(&self) -> watch::Receiver<ConnectionStatus> {
        self.0.connection_status()
    }

    async fn set_setting_values(&self, values: Vec<(SettingId, Value)>) -> Result<(), String> {
        self.0.set_setting_values(values).await.map_err(|e| e.to_string())
    }
}
