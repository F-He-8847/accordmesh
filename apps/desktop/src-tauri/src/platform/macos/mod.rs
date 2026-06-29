use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::mpsc;

use crate::platform::{
    StartedSystemAudioCapture, SystemAudioAdapter, SystemAudioCapture, SystemAudioStatus,
};
use crate::projects::types::TrackRole;
use crate::realtime::AudioFrame;

const FRAME_MAGIC: &[u8; 4] = b"AMAF";
const READY_TIMESTAMP: i64 = -1;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelperStatus {
    available: bool,
    permission_status: String,
    #[serde(default)]
    requires_restart: bool,
    #[serde(default)]
    error_code: Option<String>,
}

pub struct MacOsSystemAudioAdapter;

impl SystemAudioAdapter for MacOsSystemAudioAdapter {
    fn status(&self) -> SystemAudioStatus {
        helper_status("--probe")
    }
    fn request_permission(&self) -> SystemAudioStatus {
        helper_status("--request-permission")
    }

    fn start(
        &self,
        sender: mpsc::Sender<AudioFrame>,
    ) -> Result<StartedSystemAudioCapture, &'static str> {
        let status = self.status();
        if !status.available {
            return Err(status_error(&status));
        }
        let helper = helper_path().ok_or("ERR_SYSTEM_AUDIO_HELPER_MISSING")?;
        let mut child = Command::new(helper)
            .arg("--capture")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "ERR_SYSTEM_AUDIO_UNAVAILABLE")?;
        let mut stdout = child.stdout.take().ok_or("ERR_SYSTEM_AUDIO_UNAVAILABLE")?;
        let child = Arc::new(Mutex::new(Some(child)));
        let stopped = Arc::new(AtomicBool::new(false));
        let reader_stopped = stopped.clone();
        let (startup_tx, startup_rx) = std_mpsc::sync_channel::<Result<(), &'static str>>(1);
        let (runtime_error_tx, runtime_errors) = mpsc::unbounded_channel::<&'static str>();
        let reader = thread::spawn(move || {
            let mut startup_reported = false;
            loop {
                let mut header = [0u8; 22];
                if stdout.read_exact(&mut header).is_err() {
                    report_reader_exit(
                        startup_reported,
                        &startup_tx,
                        &runtime_error_tx,
                        &reader_stopped,
                    );
                    break;
                }
                if &header[0..4] != FRAME_MAGIC {
                    report_reader_exit(
                        startup_reported,
                        &startup_tx,
                        &runtime_error_tx,
                        &reader_stopped,
                    );
                    break;
                }
                let sample_rate = u32::from_le_bytes(header[4..8].try_into().unwrap_or([0; 4]));
                let channels = u16::from_le_bytes(header[8..10].try_into().unwrap_or([0; 2]));
                let timestamp_ms = i64::from_le_bytes(header[10..18].try_into().unwrap_or([0; 8]));
                let sample_count =
                    u32::from_le_bytes(header[18..22].try_into().unwrap_or([0; 4])) as usize;
                if timestamp_ms == READY_TIMESTAMP
                    && sample_count == 0
                    && sample_rate > 0
                    && channels > 0
                {
                    startup_reported = true;
                    let _ = startup_tx.send(Ok(()));
                    continue;
                }
                if !startup_reported
                    || sample_rate == 0
                    || channels == 0
                    || sample_count > 1_920_000
                {
                    report_reader_exit(
                        startup_reported,
                        &startup_tx,
                        &runtime_error_tx,
                        &reader_stopped,
                    );
                    break;
                }
                let mut bytes = vec![0u8; sample_count.saturating_mul(2)];
                if stdout.read_exact(&mut bytes).is_err() {
                    report_reader_exit(
                        startup_reported,
                        &startup_tx,
                        &runtime_error_tx,
                        &reader_stopped,
                    );
                    break;
                }
                let mono_pcm = bytes
                    .chunks_exact(2)
                    .map(|value| i16::from_le_bytes([value[0], value[1]]))
                    .collect::<Vec<_>>();
                if sender
                    .blocking_send(AudioFrame {
                        track_role: TrackRole::RemoteSystemAudio,
                        timestamp_ms,
                        sample_rate,
                        channels: 1,
                        mono_pcm,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        match startup_rx.recv_timeout(STARTUP_TIMEOUT) {
            Ok(Ok(())) => Ok(StartedSystemAudioCapture {
                handle: Box::new(MacOsSystemAudioCapture {
                    child,
                    reader: Some(reader),
                    stopped,
                }),
                runtime_errors,
            }),
            Ok(Err(code)) => {
                stop_process(&child, &stopped);
                let _ = reader.join();
                Err(code)
            }
            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                stop_process(&child, &stopped);
                let _ = reader.join();
                Err("ERR_SYSTEM_AUDIO_UNAVAILABLE")
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                stop_process(&child, &stopped);
                let _ = reader.join();
                Err("ERR_SYSTEM_AUDIO_START_TIMEOUT")
            }
        }
    }
}

fn report_reader_exit(
    startup_reported: bool,
    startup_tx: &std_mpsc::SyncSender<Result<(), &'static str>>,
    runtime_error_tx: &mpsc::UnboundedSender<&'static str>,
    stopped: &AtomicBool,
) {
    if stopped.load(Ordering::Relaxed) {
        return;
    }
    if startup_reported {
        let _ = runtime_error_tx.send("ERR_SYSTEM_AUDIO_RUNTIME");
    } else {
        let _ = startup_tx.send(Err("ERR_SYSTEM_AUDIO_UNAVAILABLE"));
    }
}

struct MacOsSystemAudioCapture {
    child: Arc<Mutex<Option<Child>>>,
    reader: Option<JoinHandle<()>>,
    stopped: Arc<AtomicBool>,
}

impl SystemAudioCapture for MacOsSystemAudioCapture {
    fn stop(&mut self) {
        stop_process(&self.child, &self.stopped);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for MacOsSystemAudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn stop_process(child: &Arc<Mutex<Option<Child>>>, stopped: &AtomicBool) {
    stopped.store(true, Ordering::Relaxed);
    if let Ok(mut child) = child.lock() {
        if let Some(mut process) = child.take() {
            let _ = process.kill();
            let _ = process.wait();
        }
    }
}

fn status_error(status: &SystemAudioStatus) -> &'static str {
    match status.error_code.as_deref() {
        Some("ERR_SYSTEM_AUDIO_PERMISSION_REQUIRED") => "ERR_SYSTEM_AUDIO_PERMISSION_REQUIRED",
        Some("ERR_SYSTEM_AUDIO_PERMISSION_DENIED") => "ERR_SYSTEM_AUDIO_PERMISSION_DENIED",
        Some("ERR_SYSTEM_AUDIO_RESTART_REQUIRED") => "ERR_SYSTEM_AUDIO_RESTART_REQUIRED",
        Some("ERR_SYSTEM_AUDIO_HELPER_MISSING") => "ERR_SYSTEM_AUDIO_HELPER_MISSING",
        Some("ERR_SYSTEM_AUDIO_START_TIMEOUT") => "ERR_SYSTEM_AUDIO_START_TIMEOUT",
        _ => "ERR_SYSTEM_AUDIO_UNAVAILABLE",
    }
}

fn helper_status(argument: &str) -> SystemAudioStatus {
    let Some(helper) = helper_path() else {
        return unavailable("ERR_SYSTEM_AUDIO_HELPER_MISSING", "unavailable", false);
    };
    let output = match Command::new(helper).arg(argument).output() {
        Ok(value) => value,
        Err(_) => return unavailable("ERR_SYSTEM_AUDIO_UNAVAILABLE", "unavailable", false),
    };
    let parsed = serde_json::from_slice::<HelperStatus>(&output.stdout);
    match parsed {
        Ok(value) => SystemAudioStatus {
            available: value.available,
            supported: true,
            backend: "screencapturekit".into(),
            permission_status: value.permission_status,
            device_label: "macOS system audio".into(),
            requires_restart: value.requires_restart,
            error_code: value.error_code,
        },
        Err(_) => unavailable("ERR_SYSTEM_AUDIO_UNAVAILABLE", "unavailable", false),
    }
}

fn unavailable(code: &str, permission_status: &str, requires_restart: bool) -> SystemAudioStatus {
    SystemAudioStatus {
        available: false,
        supported: true,
        backend: "screencapturekit".into(),
        permission_status: permission_status.into(),
        device_label: "macOS system audio".into(),
        requires_restart,
        error_code: Some(code.into()),
    }
}

fn helper_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("ACCORDMESH_SYSTEM_AUDIO_HELPER").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join("accordmesh-system-audio"));
            candidates.push(parent.join("../Resources/accordmesh-system-audio"));
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!(
                "accordmesh-system-audio-{}",
                env!("ACCORDMESH_TARGET_TRIPLE")
            )),
    );
    candidates.into_iter().find(|path| path.is_file())
}
