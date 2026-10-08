use std::mem::ManuallyDrop;

use windows::core::{implement, IUnknown, Interface, Ref, HRESULT};
use windows::Win32::{
    Foundation::{HANDLE, WAIT_OBJECT_0},
    Media::Audio::{
        ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
        IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_SILENT,
        AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK,
        AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
        AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
        PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
    },
    System::{
        Com::{
            IAgileObject, IAgileObject_Impl,
            StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0},
            BLOB,
        },
        Threading::{SetEvent, WaitForSingleObject},
        Variant::VT_BLOB,
    },
};

use super::engine::{float_format, Event};
use crate::{
    engine::source::SourceQueue,
    error::{AudioError, Result},
};

const ACTIVATE_TIMEOUT_MS: u32 = 2000;
/// Loopback buffer (100 ms): sources are polled once per mic cycle, which can be ~20 ms apart.
const BUFFER: i64 = 1_000_000;

/// Signals an event when Windows finishes activating the loopback client.
#[implement(IActivateAudioInterfaceCompletionHandler, IAgileObject)]
struct Activated(HANDLE);

impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
    fn ActivateCompleted(&self, _: Ref<IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        unsafe { SetEvent(self.0) }
    }
}

impl IAgileObject_Impl for Activated_Impl {}

/// An audio client that hears process `pid` and its children, wherever they play.
fn activate(pid: u32) -> Result<IAudioClient> {
    unsafe {
        let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: pid,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                },
            },
        };
        // Never dropped: PropVariantClear would try to free `params`, which lives on this stack.
        let prop = ManuallyDrop::new(PROPVARIANT {
            Anonymous: PROPVARIANT_0 {
                Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                    vt: VT_BLOB,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: PROPVARIANT_0_0_0 {
                        blob: BLOB {
                            cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                            pBlobData: &mut params as *mut _ as *mut u8,
                        },
                    },
                }),
            },
        });
        let done = Event::new()?;
        let handler: IActivateAudioInterfaceCompletionHandler = Activated(done.0).into();
        let operation =
            ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&*prop), &handler)?;
        if WaitForSingleObject(done.0, ACTIVATE_TIMEOUT_MS) != WAIT_OBJECT_0 {
            return Err(AudioError::Os("Windows didn't open the app's audio in time".into()));
        }
        let mut result = HRESULT(0);
        let mut unknown: Option<IUnknown> = None;
        operation.GetActivateResult(&mut result, &mut unknown)?;
        result.ok()?;
        let unknown = unknown.ok_or_else(|| AudioError::Os("Windows returned no audio client".into()))?;
        Ok(unknown.cast()?)
    }
}

/// Captures one process tree's audio, downmixed to mono.
pub struct LoopbackCapture {
    client: IAudioClient,
    capture: IAudioCaptureClient,
}

// SAFETY: created on an MTA thread (the audio worker) and used and dropped only on MTA threads (the engine
// thread); WASAPI clients are free-threaded.
unsafe impl Send for LoopbackCapture {}

impl LoopbackCapture {
    pub fn open(pid: u32) -> Result<Self> {
        unsafe {
            let client = activate(pid)?;
            let format = float_format(2);
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                BUFFER,
                0,
                &format,
                None,
            )?;
            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            Ok(Self { client, capture })
        }
    }

    /// Moves everything captured so far into `queue` as mono. `scratch` must hold one packet (100 ms).
    pub fn drain(&self, queue: &mut SourceQueue, scratch: &mut [f32]) -> Result<()> {
        unsafe {
            loop {
                if self.capture.GetNextPacketSize()? == 0 {
                    return Ok(());
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let n = (frames as usize).min(scratch.len());
                if data.is_null() || flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                    scratch[..n].fill(0.0);
                } else {
                    let stereo = std::slice::from_raw_parts(data as *const f32, n * 2);
                    for (m, pair) in scratch[..n].iter_mut().zip(stereo.as_chunks::<2>().0) {
                        *m = (pair[0] + pair[1]) * 0.5;
                    }
                }
                queue.push(&scratch[..n]);
                self.capture.ReleaseBuffer(frames)?;
            }
        }
    }
}

impl Drop for LoopbackCapture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

/// Whether this Windows supports capturing single apps (tries it on this process).
pub fn supported() -> bool {
    activate(std::process::id()).is_ok()
}
