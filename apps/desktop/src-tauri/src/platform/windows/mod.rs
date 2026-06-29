use tokio::sync::mpsc;

use crate::platform::{StartedSystemAudioCapture, SystemAudioAdapter, SystemAudioStatus};
use crate::realtime::AudioFrame;

/// Windows production support will implement this contract with WASAPI loopback.
/// Keeping the backend explicit and unavailable prevents the macOS implementation
/// from leaking platform types into the shared realtime/domain layers.
pub struct WindowsLoopbackAudioAdapter;

impl SystemAudioAdapter for WindowsLoopbackAudioAdapter {
    fn status(&self) -> SystemAudioStatus {
        SystemAudioStatus {
            available: false,
            supported: false,
            backend: "wasapi_loopback".into(),
            permission_status: "not_implemented".into(),
            device_label: "Windows system audio".into(),
            requires_restart: false,
            error_code: Some("ERR_SYSTEM_AUDIO_BACKEND_NOT_IMPLEMENTED".into()),
        }
    }
    fn start(
        &self,
        _sender: mpsc::Sender<AudioFrame>,
    ) -> Result<StartedSystemAudioCapture, &'static str> {
        Err("ERR_SYSTEM_AUDIO_BACKEND_NOT_IMPLEMENTED")
    }
}
