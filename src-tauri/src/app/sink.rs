use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use super::tray;
use crate::{
    error::AudioError,
    model::{MeterFrame, UiState},
    worker::EventSink,
};

/// Notices raised before any window was listening; drained by `take_notices`.
#[derive(Default)]
pub struct Notices(pub Mutex<Vec<AudioError>>);

pub fn push_notice(app: &AppHandle, error: AudioError) {
    if let Ok(mut n) = app.state::<Notices>().0.lock() {
        n.push(error.clone());
    }
    let _ = app.emit("notice", error);
}

pub struct TauriSink {
    pub app: AppHandle,
}

impl EventSink for TauriSink {
    fn state(&self, state: &UiState) {
        let _ = self.app.emit("state", state);
    }

    fn meters(&self, frame: &MeterFrame) {
        let _ = self.app.emit("meters", frame);
    }

    fn mic_muted(&self, muted: bool) {
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || tray::set_mic_muted(&app, muted));
        let _ = self.app.emit("mic-muted", muted);
    }

    fn notice(&self, error: &AudioError) {
        push_notice(&self.app, error.clone());
    }
}

pub struct HeadsetEvents {
    pub app: AppHandle,
}

impl crate::headset::HeadsetSink for HeadsetEvents {
    fn state(&self, state: &crate::headset::model::HeadsetState) {
        let _ = self.app.emit("headset", state);
    }
}
