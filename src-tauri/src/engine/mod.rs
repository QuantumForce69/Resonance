//! Phase 3a processed microphone: settings, DSP, clock-drift handling and the engine interface.
pub mod config;
pub mod drift;
pub mod dsp;
pub mod feed;
pub mod fifo;
pub mod source;

pub use config::{
    CallSettings, CompressorParams, DenoiseParams, DspParams, EngineStatus, GateParams, LimiterParams, ProcessingSettings,
    ProcessingView, RestoreDefaults,
};

use crate::error::{AudioError, Result};

/// Where the engine reads, writes and (optionally) monitors.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub source_id: String,
    /// VB-Cable's render endpoint ("CABLE Input").
    pub cable_id: String,
    pub monitor_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineParams {
    pub dsp: DspParams,
    pub monitor_level: f32,
    pub call: CallSettings,
}

/// One process captured into the call, at a send level.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpec {
    /// The app it belongs to (for messages).
    pub key: String,
    pub pid: u32,
    pub level: f32,
}

/// The real-time processing engine (WASAPI in production, a fake in tests). Owned by the audio worker.
pub trait AudioEngine: Send {
    /// Opens the devices and starts processing; returns once audio flows or the devices failed to open.
    fn start(&mut self, route: &Route, params: &EngineParams) -> Result<()>;
    fn stop(&mut self);
    /// Applies new settings to the running engine without interrupting audio.
    fn set_params(&mut self, params: &EngineParams);
    /// Why a started engine stopped by itself (device unplugged, driver error), if it did.
    fn failure(&self) -> Option<String>;
    /// Highest linear peaks before and after processing since the last call.
    fn take_peaks(&self) -> (f32, f32);
    /// Makes the running engine capture exactly these processes into the call. Returns problems to show
    /// (a capture that couldn't open); those processes are skipped.
    fn set_sources(&mut self, sources: &[SourceSpec]) -> Vec<AudioError>;
    /// Whether Windows can capture single apps (process loopback).
    fn supports_app_capture(&self) -> bool;
}

/// Stands in until a real engine is installed; processing reports itself unavailable.
pub struct NullEngine;

impl AudioEngine for NullEngine {
    fn start(&mut self, _: &Route, _: &EngineParams) -> Result<()> {
        Err(AudioError::Unavailable("Audio processing isn't available".into()))
    }
    fn stop(&mut self) {}
    fn set_params(&mut self, _: &EngineParams) {}
    fn failure(&self) -> Option<String> {
        None
    }
    fn take_peaks(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn set_sources(&mut self, _: &[SourceSpec]) -> Vec<AudioError> {
        Vec::new()
    }
    fn supports_app_capture(&self) -> bool {
        false
    }
}
