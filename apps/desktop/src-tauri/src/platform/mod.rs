use serde::Serialize;
use tokio::sync::mpsc;

use crate::realtime::AudioFrame;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAudioStatus {
    pub available: bool,
    pub supported: bool,
    pub backend: String,
    pub permission_status: String,
    pub device_label: String,
    pub requires_restart: bool,
    pub error_code: Option<String>,
}

pub struct StartedSystemAudioCapture {
    pub handle: Box<dyn SystemAudioCapture>,
    pub runtime_errors: mpsc::UnboundedReceiver<&'static str>,
}

pub trait SystemAudioCapture: Send {
    fn stop(&mut self);
}
pub trait SystemAudioAdapter {
    fn status(&self) -> SystemAudioStatus;
    fn request_permission(&self) -> SystemAudioStatus {
        self.status()
    }
    fn start(
        &self,
        sender: mpsc::Sender<AudioFrame>,
    ) -> Result<StartedSystemAudioCapture, &'static str>;
}

pub fn adapter() -> Box<dyn SystemAudioAdapter> {
    #[cfg(target_os = "macos")]
    {
        return Box::new(macos::MacOsSystemAudioAdapter);
    }
    #[cfg(target_os = "windows")]
    {
        return Box::new(windows::WindowsLoopbackAudioAdapter);
    }
    #[allow(unreachable_code)]
    Box::new(UnsupportedSystemAudioAdapter)
}

struct UnsupportedSystemAudioAdapter;
impl SystemAudioAdapter for UnsupportedSystemAudioAdapter {
    fn status(&self) -> SystemAudioStatus {
        SystemAudioStatus {
            available: false,
            supported: false,
            backend: "unsupported".into(),
            permission_status: "unavailable".into(),
            device_label: "system_audio".into(),
            requires_restart: false,
            error_code: Some("ERR_SYSTEM_AUDIO_UNSUPPORTED".into()),
        }
    }
    fn start(
        &self,
        _: mpsc::Sender<AudioFrame>,
    ) -> Result<StartedSystemAudioCapture, &'static str> {
        Err("ERR_SYSTEM_AUDIO_UNSUPPORTED")
    }
}
