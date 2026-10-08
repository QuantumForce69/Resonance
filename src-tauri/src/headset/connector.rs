use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use openscq30_lib::{storage::PairedDevice, DeviceModel, OpenSCQ30Session};

use super::device::{Found, HeadsetConnector, LibDevice};

const MODEL: DeviceModel = DeviceModel::SoundcoreA3035;

/// Connects through openscq30-lib: list Space Ones Windows knows about, register the first with the
/// library's own store if needed, and connect. `demo` uses the library's simulated device.
pub struct LibConnector {
    session: OpenSCQ30Session,
    demo: bool,
}

impl LibConnector {
    pub async fn open(db_path: PathBuf, demo: bool) -> Result<Self, String> {
        if let Some(dir) = db_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let session = match OpenSCQ30Session::new(db_path.clone()).await {
            Ok(s) => s,
            Err(e) => {
                let backup = crate::paths::quarantine(&db_path);
                tracing::warn!("headset database unusable ({e}); moved to {}", backup.display());
                OpenSCQ30Session::new(db_path).await.map_err(|e| e.to_string())?
            }
        };
        Ok(Self { session, demo })
    }
}

#[async_trait]
impl HeadsetConnector for LibConnector {
    async fn connect(&self) -> Result<Option<Found>, String> {
        let listed = if self.demo {
            self.session.list_demo_devices(MODEL).await
        } else {
            self.session.list_devices(MODEL).await
        }
        .map_err(|e| e.to_string())?;
        let Some(descriptor) = listed.into_iter().next() else { return Ok(None) };
        let paired = self.session.paired_devices().await.map_err(|e| e.to_string())?;
        if !paired.iter().any(|p| p.mac_address == descriptor.mac_address && p.model == MODEL) {
            self.session
                .pair(PairedDevice { mac_address: descriptor.mac_address, model: MODEL, is_demo: self.demo })
                .await
                .map_err(|e| e.to_string())?;
        }
        let device = self.session.connect(descriptor.mac_address).await.map_err(|e| e.to_string())?;
        Ok(Some(Found { name: descriptor.name, device: Arc::new(LibDevice(device)) }))
    }
}
