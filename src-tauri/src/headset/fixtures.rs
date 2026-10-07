//! Settings exactly as openscq30-lib v2.12.0 reports them for a Space One (captured from its demo device).
use std::collections::HashMap;

use openscq30_lib::settings::{Setting, SettingId};

const SETTINGS: &[(SettingId, &str)] = &[
    (SettingId::AmbientSoundMode, r#"{"type":"select","setting":{"options":["NoiseCanceling","Transparency","Normal"],"localizedOptions":["Noise Canceling","Transparency","Normal"]},"value":"NoiseCanceling"}"#),
    (SettingId::NoiseCancelingMode, r#"{"type":"select","setting":{"options":["Custom","Adaptive"],"localizedOptions":["Custom","Adaptive"]},"value":"Custom"}"#),
    (SettingId::AdaptiveNoiseCanceling, r#"{"type":"information","value":"3/5","translatedValue":"3/5"}"#),
    (SettingId::ManualNoiseCanceling, r#"{"type":"i32Range","setting":{"start":1,"end":5,"step":1},"value":4}"#),
    (SettingId::ManualTransparency, r#"{"type":"i32Range","setting":{"start":1,"end":5,"step":1},"value":2}"#),
    (SettingId::WindNoiseSuppression, r#"{"type":"toggle","value":true}"#),
    (SettingId::BatteryLevel, r#"{"type":"information","value":"4/5","translatedValue":"80%"}"#),
    (SettingId::FirmwareVersion, r#"{"type":"information","value":"06.87","translatedValue":"06.87"}"#),
    (SettingId::SerialNumber, r#"{"type":"information","value":"3035705028ABCDEF","translatedValue":"3035705028ABCDEF"}"#),
    (SettingId::PresetEqualizerProfile, r#"{"type":"presetEqualizerProfileSelect","equalizer":{"bandHz":[100,200,400,800,1600,3200,6400,12800],"fractionDigits":1,"min":-60,"max":60},"select":{"options":["SoundcoreSignature","Acoustic","Flat"],"localizedOptions":["Soundcore Signature","Acoustic","Flat"]},"presets":[[0,0,0,0,0,0,0,0],[40,10,20,20,40,40,40,20],[0,0,0,0,0,0,0,0]],"value":"SoundcoreSignature"}"#),
    (SettingId::VolumeAdjustments, r#"{"type":"equalizer","setting":{"bandHz":[100,200,400,800,1600,3200,6400,12800],"fractionDigits":1,"min":-120,"max":134},"readOnly":false,"value":[0,0,0,0,0,0,0,0]}"#),
];

pub fn space_one_settings() -> HashMap<SettingId, Setting> {
    SETTINGS.iter().map(|(id, json)| (*id, serde_json::from_str(json).expect("valid setting json"))).collect()
}

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use openscq30_lib::{connection::ConnectionStatus, settings::Value};
use tokio::sync::{mpsc, watch};

use super::{
    device::{Found, HeadsetConnector, HeadsetDevice},
    manager::HeadsetSink,
    mapping::SettingSource,
    model::HeadsetState,
};

pub struct FakeDevice {
    pub settings: Mutex<HashMap<SettingId, Setting>>,
    pub writes: Mutex<Vec<Vec<(SettingId, Value)>>>,
    pub changes: watch::Sender<()>,
    pub status: watch::Sender<ConnectionStatus>,
}

impl FakeDevice {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            settings: Mutex::new(space_one_settings()),
            writes: Mutex::new(Vec::new()),
            changes: watch::channel(()).0,
            status: watch::channel(ConnectionStatus::Connected).0,
        })
    }
}

impl SettingSource for FakeDevice {
    fn setting(&self, id: &SettingId) -> Option<Setting> {
        self.settings.lock().unwrap().get(id).cloned()
    }
}

#[async_trait]
impl HeadsetDevice for FakeDevice {
    fn watch_for_changes(&self) -> watch::Receiver<()> {
        self.changes.subscribe()
    }

    fn connection_status(&self) -> watch::Receiver<ConnectionStatus> {
        self.status.subscribe()
    }

    async fn set_setting_values(&self, values: Vec<(SettingId, Value)>) -> Result<(), String> {
        {
            let mut settings = self.settings.lock().unwrap();
            for (id, value) in &values {
                match (settings.get_mut(id), value) {
                    (Some(Setting::Select { value: v, .. }), Value::String(s)) => *v = s.clone(),
                    (Some(Setting::Equalizer { value: v, .. }), Value::I16Vec(new)) => *v = new.clone(),
                    _ => {}
                }
            }
        }
        self.writes.lock().unwrap().push(values);
        let _ = self.changes.send(());
        Ok(())
    }
}

pub struct FakeConnector {
    pub results: Mutex<VecDeque<Result<Option<Arc<FakeDevice>>, String>>>,
    pub calls: Mutex<usize>,
}

impl FakeConnector {
    pub fn new(results: Vec<Result<Option<Arc<FakeDevice>>, String>>) -> Arc<Self> {
        Arc::new(Self { results: Mutex::new(results.into()), calls: Mutex::new(0) })
    }
}

#[async_trait]
impl HeadsetConnector for FakeConnector {
    async fn connect(&self) -> Result<Option<Found>, String> {
        *self.calls.lock().unwrap() += 1;
        let next = self.results.lock().unwrap().pop_front().unwrap_or(Ok(None));
        next.map(|o| o.map(|d| Found { name: "soundcore Space One".into(), device: d }))
    }
}

pub struct ChanSink(pub mpsc::UnboundedSender<HeadsetState>);

impl HeadsetSink for ChanSink {
    fn state(&self, s: &HeadsetState) {
        let _ = self.0.send(s.clone());
    }
}
