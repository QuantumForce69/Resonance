use windows::Win32::{
    Media::Audio::{Endpoints::IAudioMeterInformation, IAudioCaptureClient, IAudioClient, IMMDevice, AUDCLNT_SHAREMODE_SHARED},
    System::Com::{CoTaskMemFree, CLSCTX_ALL},
};

use crate::error::Result;

/// Capture endpoints only report peaks while a stream is open, so this opens a shared-mode
/// stream and discards the data. Alive only while a Resonance window is visible.
pub struct MicMeter {
    pub device_id: String,
    client: IAudioClient,
    capture: IAudioCaptureClient,
    meter: IAudioMeterInformation,
}

impl MicMeter {
    pub fn open(dev: &IMMDevice, id: &str) -> Result<Self> {
        unsafe {
            let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
            let format = client.GetMixFormat()?;
            let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 10_000_000, 0, format, None);
            CoTaskMemFree(Some(format as *const _));
            init?;
            let capture: IAudioCaptureClient = client.GetService()?;
            let meter: IAudioMeterInformation = dev.Activate(CLSCTX_ALL, None)?;
            client.Start()?;
            Ok(Self { device_id: id.to_string(), client, capture, meter })
        }
    }

    pub fn peak(&self) -> f32 {
        unsafe {
            // Drain and discard captured packets so the buffer never overflows.
            while let Ok(n) = self.capture.GetNextPacketSize() {
                if n == 0 {
                    break;
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                if self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None).is_err() {
                    break;
                }
                let _ = self.capture.ReleaseBuffer(frames);
            }
            self.meter.GetPeakValue().unwrap_or(0.0)
        }
    }
}

impl Drop for MicMeter {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}
