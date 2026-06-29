use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::audio::{self, ActiveCapture};
use crate::crypto;
use crate::jobs::{persist_generation, persist_realtime_chunk_generation};
use crate::media::TemporaryPath;
use crate::platform::{self, SystemAudioCapture};
use crate::projects::types::*;
use crate::providers::{GenerationInput, Provider, ProviderContext, TranscriptDraft};
use crate::storage::repository::Repository;

const REALTIME_SPOOL_VERSION: u16 = 2;
const LIVE_TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(12);
const LIVE_WORKER_ABORT_GRACE: Duration = Duration::from_millis(500);
const REALTIME_SPOOL_MAGIC: &[u8; 4] = b"AMSP";

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub track_role: TrackRole,
    pub timestamp_ms: i64,
    pub sample_rate: u32,
    pub channels: u16,
    pub mono_pcm: Vec<i16>,
}

fn default_spool_version() -> u16 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRealtimeChunk {
    #[serde(default = "default_spool_version")]
    pub format_version: u16,
    pub id: String,
    pub track_role: TrackRole,
    pub start_ms: i64,
    #[serde(default)]
    pub end_ms: i64,
    pub sample_rate: u32,
    pub encrypted_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeSpoolSession {
    pub format_version: u16,
    pub project_id: String,
    pub session_id: String,
    pub provider_id: String,
    pub source_language: Option<String>,
    pub translation_language: Option<String>,
    pub output_language: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RealtimeSpoolEnvelopeHeader {
    format_version: u16,
    id: String,
    track_role: TrackRole,
    start_ms: i64,
    end_ms: i64,
    sample_rate: u32,
}

enum LiveWork {
    Partial {
        track_role: TrackRole,
        start_ms: i64,
        sample_rate: u32,
        samples: Vec<i16>,
    },
    Final(PendingRealtimeChunk),
}

pub struct ActiveRealtime {
    pub session_id: String,
    pub project_id: String,
    pub provider_id: String,
    pub output_language: String,
    pub cancelled: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    interrupted: Arc<AtomicBool>,
    analyze_generation: Arc<AtomicU64>,
    completion: Arc<(Mutex<bool>, Condvar)>,
    terminal_error: Arc<Mutex<Option<&'static str>>>,
    pending_chunks: Arc<Mutex<Vec<PendingRealtimeChunk>>>,
    accept_live_results: Arc<AtomicBool>,
    live_worker: Option<JoinHandle<()>>,
    pub source_language: Option<String>,
    pub translation_language: Option<String>,
    microphone: Option<ActiveCapture>,
    system_audio: Option<Box<dyn SystemAudioCapture>>,
}
impl ActiveRealtime {
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }
    pub fn analyze_now(&self) {
        self.analyze_generation.fetch_add(1, Ordering::Relaxed);
    }
    pub fn is_completed(&self) -> bool {
        self.completion
            .0
            .lock()
            .map(|value| *value)
            .unwrap_or(false)
    }
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::Relaxed)
    }
    pub fn is_active(&self) -> bool {
        !self.is_completed() && !self.is_interrupted()
    }
    pub fn cleanup_pending(&self) -> bool {
        self.is_interrupted() && !self.is_completed()
    }
    pub fn terminal_error(&self) -> Option<&'static str> {
        self.terminal_error.lock().ok().and_then(|value| *value)
    }
    pub fn take_pending_chunks(&self) -> Vec<PendingRealtimeChunk> {
        self.pending_chunks
            .lock()
            .map(|mut chunks| std::mem::take(&mut *chunks))
            .unwrap_or_default()
    }

    fn stop_remote_worker(&mut self) {
        self.accept_live_results.store(false, Ordering::Release);
        self.cancelled.store(true, Ordering::Release);
        if let Some(worker) = self.live_worker.take() {
            worker.abort();
            let deadline = Instant::now() + LIVE_WORKER_ABORT_GRACE;
            while !worker.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    pub fn mark_interrupted(&mut self) {
        self.interrupted.store(true, Ordering::Relaxed);
        self.stopping.store(true, Ordering::Relaxed);
        self.stop_remote_worker();
        self.microphone.take();
        if let Some(mut capture) = self.system_audio.take() {
            capture.stop();
        }
    }

    pub fn stop_and_wait(&mut self, timeout: Duration) -> bool {
        self.stopping.store(true, Ordering::Release);
        self.stop_remote_worker();
        self.microphone.take();
        if let Some(mut capture) = self.system_audio.take() {
            capture.stop();
        }
        let (lock, signal) = &*self.completion;
        let completed = match lock.lock() {
            Ok(value) => value,
            Err(_) => return false,
        };
        match signal.wait_timeout_while(completed, timeout, |value| !*value) {
            Ok((value, _)) => *value,
            Err(_) => false,
        }
    }
}

pub struct StartOptions {
    pub device_id: String,
    pub mode: RealtimeMode,
    pub provider_id: String,
    pub source_language: Option<String>,
    pub translation_language: Option<String>,
    pub analysis_language: String,
}

pub fn start(
    app: AppHandle,
    repo: Repository,
    master_key: Zeroizing<Vec<u8>>,
    project: MeetingProject,
    session: RealtimeSession,
    provider: Arc<dyn Provider>,
    options: StartOptions,
) -> Result<ActiveRealtime, &'static str> {
    let active_project_id = project.id.clone();
    let project_key = repo
        .project_key(&project.id, &master_key)
        .map_err(|_| "ERR_CRYPTO")?;
    drop(master_key);
    let cancelled = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let interrupted = Arc::new(AtomicBool::new(false));
    let analyze_generation = Arc::new(AtomicU64::new(0));
    let completion = Arc::new((Mutex::new(false), Condvar::new()));
    let terminal_error = Arc::new(Mutex::new(None));
    let pending_chunks = Arc::new(Mutex::new(Vec::new()));
    let accept_live_results = Arc::new(AtomicBool::new(true));
    let remote_deferred = Arc::new(AtomicBool::new(false));

    let (rx, microphone, runtime_errors, system_audio, live_tx, live_rx) =
        if provider.id() == "mock" {
            (None, None, None, None, None, None)
        } else {
            let (tx, rx) = mpsc::channel(64);
            let microphone_role = if matches!(options.mode, RealtimeMode::Online) {
                TrackRole::LocalMicrophone
            } else {
                TrackRole::RoomMicrophone
            };
            let started = audio::start_capture(&options.device_id, microphone_role, tx.clone())?;
            let system_started = if matches!(options.mode, RealtimeMode::Online) {
                Some(platform::adapter().start(tx)?)
            } else {
                None
            };
            let (error_tx, error_rx) = mpsc::unbounded_channel();
            forward_runtime_errors(started.runtime_errors, error_tx.clone());
            let system_audio = system_started.map(|started| {
                forward_runtime_errors(started.runtime_errors, error_tx.clone());
                started.handle
            });
            drop(error_tx);
            let (live_tx, live_rx) = mpsc::channel(16);
            (
                Some(rx),
                Some(started.handle),
                Some(error_rx),
                system_audio,
                Some(live_tx),
                Some(live_rx),
            )
        };

    write_spool_session(
        &repo,
        &RealtimeSpoolSession {
            format_version: REALTIME_SPOOL_VERSION,
            project_id: project.id.clone(),
            session_id: session.id.clone(),
            provider_id: options.provider_id.clone(),
            source_language: options.source_language.clone(),
            translation_language: options.translation_language.clone(),
            output_language: options.analysis_language.clone(),
            created_at: Utc::now().to_rfc3339(),
        },
    )?;

    let live_worker = live_rx.map(|receiver| {
        let worker_app = app.clone();
        let worker_repo = repo.clone();
        let worker_project_id = project.id.clone();
        let worker_provider_id = options.provider_id.clone();
        let worker_key = project_key.clone();
        let worker_provider = provider.clone();
        let worker_translation = options.translation_language.clone();
        let worker_analysis = options.analysis_language.clone();
        let worker_context = ProviderContext {
            cancelled: cancelled.clone(),
            source_language: options.source_language.clone(),
            model_override: None,
        };
        let worker_accept = accept_live_results.clone();
        let worker_deferred = remote_deferred.clone();
        tauri::async_runtime::handle().inner().spawn(async move {
            run_live_processor(
                worker_app,
                worker_repo,
                worker_project_id,
                worker_provider_id,
                worker_key,
                worker_provider,
                worker_translation,
                worker_analysis,
                worker_context,
                worker_accept,
                worker_deferred,
                receiver,
            )
            .await;
        })
    });

    let runner_cancel = cancelled.clone();
    let runner_pause = paused.clone();
    let runner_stop = stopping.clone();
    let runner_interrupted = interrupted.clone();
    let runner_analyze = analyze_generation.clone();
    let runner_completion = completion.clone();
    let runner_error = terminal_error.clone();
    let runner_pending = pending_chunks.clone();
    let runner_deferred = remote_deferred.clone();
    let provider_id = options.provider_id.clone();
    let runtime_provider_id = provider_id.clone();
    let runtime_language = options.analysis_language.clone();
    let runtime_source_language = options.source_language.clone();
    let runtime_translation_language = options.translation_language.clone();
    let session_id = session.id.clone();
    let mock_track = if matches!(options.mode, RealtimeMode::Online) {
        TrackRole::RemoteSystemAudio
    } else {
        TrackRole::RoomMicrophone
    };
    tauri::async_runtime::spawn(async move {
        let context = ProviderContext {
            cancelled: runner_cancel.clone(),
            source_language: options.source_language.clone(),
            model_override: None,
        };
        let result = if let Some(rx) = rx {
            run_audio(
                app.clone(),
                repo.clone(),
                project.id.clone(),
                project_key,
                context,
                runner_pause,
                runner_stop.clone(),
                runner_analyze,
                runner_pending,
                runtime_errors.expect("real capture error channel"),
                rx,
                live_tx.expect("real capture live queue"),
                runner_deferred,
            )
            .await
        } else {
            run_mock(
                app.clone(),
                repo.clone(),
                project.id.clone(),
                provider_id,
                project_key,
                provider,
                options.translation_language,
                options.analysis_language,
                context,
                runner_pause,
                runner_stop.clone(),
                mock_track,
            )
            .await
        };
        if let Err(code) = result {
            let clean_stop_cancel =
                code == "ERR_JOB_CANCELLED" && runner_stop.load(Ordering::Relaxed);
            if !clean_stop_cancel {
                if let Ok(mut error) = runner_error.lock() {
                    *error = Some(code);
                }
                repo.fail_realtime_session(&session_id, &project.id).ok();
            }
            if !clean_stop_cancel && !runner_interrupted.load(Ordering::Relaxed) {
                app.emit(
                    "accordmesh://realtime-error",
                    json!({"projectId":project.id,"errorCode":code}),
                )
                .ok();
            }
        }
        let (lock, signal) = &*runner_completion;
        if let Ok(mut completed) = lock.lock() {
            *completed = true;
            signal.notify_all();
        }
    });
    Ok(ActiveRealtime {
        session_id: session.id,
        project_id: active_project_id,
        provider_id: runtime_provider_id,
        output_language: runtime_language,
        cancelled,
        paused,
        stopping,
        interrupted,
        analyze_generation,
        completion,
        terminal_error,
        pending_chunks,
        accept_live_results,
        live_worker,
        source_language: runtime_source_language,
        translation_language: runtime_translation_language,
        microphone,
        system_audio,
    })
}

#[cfg(test)]
pub(crate) fn synthetic_pending(session_id: &str, project_id: &str) -> ActiveRealtime {
    ActiveRealtime {
        session_id: session_id.to_owned(),
        project_id: project_id.to_owned(),
        provider_id: "mock".into(),
        output_language: "en".into(),
        cancelled: Arc::new(AtomicBool::new(false)),
        paused: Arc::new(AtomicBool::new(false)),
        stopping: Arc::new(AtomicBool::new(false)),
        interrupted: Arc::new(AtomicBool::new(false)),
        analyze_generation: Arc::new(AtomicU64::new(0)),
        completion: Arc::new((Mutex::new(false), Condvar::new())),
        terminal_error: Arc::new(Mutex::new(None)),
        pending_chunks: Arc::new(Mutex::new(Vec::new())),
        accept_live_results: Arc::new(AtomicBool::new(true)),
        live_worker: None,
        source_language: None,
        translation_language: None,
        microphone: None,
        system_audio: None,
    }
}

#[cfg(test)]
pub(crate) fn complete_synthetic(runtime: &ActiveRealtime) {
    let (lock, signal) = &*runtime.completion;
    if let Ok(mut completed) = lock.lock() {
        *completed = true;
        signal.notify_all();
    }
}

#[cfg(test)]
pub(crate) fn attach_live_worker_for_test(runtime: &mut ActiveRealtime, worker: JoinHandle<()>) {
    runtime.live_worker = Some(worker);
}

fn forward_runtime_errors(
    mut source: mpsc::UnboundedReceiver<&'static str>,
    target: mpsc::UnboundedSender<&'static str>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(code) = source.recv().await {
            if target.send(code).is_err() {
                break;
            }
        }
    });
}

async fn run_mock(
    app: AppHandle,
    repo: Repository,
    project_id: String,
    provider_id: String,
    key: Zeroizing<Vec<u8>>,
    provider: Arc<dyn Provider>,
    translation: Option<String>,
    analysis: String,
    context: ProviderContext,
    paused: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    track: TrackRole,
) -> Result<(), &'static str> {
    let drafts = provider
        .transcribe_realtime_chunk(Path::new("mock-realtime"), 0, track, &context)
        .await?;
    for draft in drafts {
        while paused.load(Ordering::Relaxed) && !stopping.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if stopping.load(Ordering::Relaxed) {
            return Ok(());
        }
        app.emit("accordmesh://timeline-partial",json!({"projectId":project_id,"sourceTranscript":draft.text,"startMs":draft.start_ms,"transcriptStatus":"partial"})).ok();
        tokio::time::sleep(Duration::from_millis(850)).await;
        persist_final(
            &app,
            &repo,
            &project_id,
            &provider_id,
            &key,
            provider.as_ref(),
            draft,
            track,
            translation.as_deref(),
            &analysis,
            &context,
        )
        .await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    while !stopping.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Ok(())
}

struct TrackBuffer {
    samples: Vec<i16>,
    sample_rate: u32,
    start_ms: i64,
    last_voice_ms: i64,
    last_partial_len: usize,
    last_analyze_generation: u64,
}

#[allow(clippy::too_many_arguments)]
async fn run_audio(
    app: AppHandle,
    repo: Repository,
    project_id: String,
    key: Zeroizing<Vec<u8>>,
    context: ProviderContext,
    paused: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    analyze_generation: Arc<AtomicU64>,
    pending_chunks: Arc<Mutex<Vec<PendingRealtimeChunk>>>,
    mut runtime_errors: mpsc::UnboundedReceiver<&'static str>,
    mut rx: mpsc::Receiver<AudioFrame>,
    live_tx: mpsc::Sender<LiveWork>,
    remote_deferred: Arc<AtomicBool>,
) -> Result<(), &'static str> {
    let mut tracks: HashMap<TrackRole, TrackBuffer> = HashMap::new();
    while let Some(frame) = receive_audio_frame(&mut rx, &mut runtime_errors, &stopping).await? {
        if context.cancelled.load(Ordering::Relaxed) {
            if stopping.load(Ordering::Relaxed) {
                break;
            }
            return Err("ERR_JOB_CANCELLED");
        }
        if paused.load(Ordering::Relaxed) {
            continue;
        }
        let rms = (frame
            .mono_pcm
            .iter()
            .map(|v| {
                let x = *v as f64 / 32768.0;
                x * x
            })
            .sum::<f64>()
            / frame.mono_pcm.len().max(1) as f64)
            .sqrt();
        let voiced = rms > 0.018;
        let buffer = tracks
            .entry(frame.track_role)
            .or_insert_with(|| TrackBuffer {
                samples: Vec::new(),
                sample_rate: frame.sample_rate,
                start_ms: frame.timestamp_ms,
                last_voice_ms: frame.timestamp_ms,
                last_partial_len: 0,
                last_analyze_generation: 0,
            });
        if voiced || !buffer.samples.is_empty() {
            buffer.samples.extend_from_slice(&frame.mono_pcm);
        }
        if voiced {
            buffer.last_voice_ms = frame.timestamp_ms;
        }
        let length_ms = buffer.samples.len() as i64 * 1000 / buffer.sample_rate.max(1) as i64;
        let generation = analyze_generation.load(Ordering::Relaxed);
        let force = consume_analyze_generation(&mut buffer.last_analyze_generation, generation);
        let final_ready = !buffer.samples.is_empty()
            && (force || frame.timestamp_ms - buffer.last_voice_ms > 800 || length_ms > 20_000);
        let partial_ready = length_ms > 3000
            && buffer.samples.len().saturating_sub(buffer.last_partial_len)
                > buffer.sample_rate as usize * 3;

        if partial_ready && !final_ready && !remote_deferred.load(Ordering::Acquire) {
            let item = LiveWork::Partial {
                track_role: frame.track_role,
                start_ms: buffer.start_ms,
                sample_rate: buffer.sample_rate,
                samples: buffer.samples.clone(),
            };
            if live_tx.try_send(item).is_ok() {
                buffer.last_partial_len = buffer.samples.len();
            }
        }

        if final_ready {
            let samples = std::mem::take(&mut buffer.samples);
            let start = buffer.start_ms;
            buffer.start_ms = frame.timestamp_ms;
            buffer.last_partial_len = 0;
            let path = write_wav(
                &repo,
                &project_id,
                frame.track_role,
                buffer.sample_rate,
                &samples,
                "final",
            )
            .await?;
            let chunk = stage_pending_wav(
                &repo,
                &project_id,
                &key,
                &path,
                start,
                frame.track_role,
                buffer.sample_rate,
            )
            .await?;
            push_pending_chunk(&pending_chunks, chunk.clone())?;
            if !stopping.load(Ordering::Acquire) && !remote_deferred.load(Ordering::Acquire) {
                if live_tx.try_send(LiveWork::Final(chunk)).is_err() {
                    app.emit(
                        "accordmesh://realtime-deferred",
                        json!({"projectId":project_id}),
                    )
                    .ok();
                }
            }
        }
    }

    if stopping.load(Ordering::Relaxed) {
        for (track, buffer) in tracks {
            if buffer.samples.is_empty() {
                continue;
            }
            let path = write_wav(
                &repo,
                &project_id,
                track,
                buffer.sample_rate,
                &buffer.samples,
                "stop",
            )
            .await?;
            let chunk = stage_pending_wav(
                &repo,
                &project_id,
                &key,
                &path,
                buffer.start_ms,
                track,
                buffer.sample_rate,
            )
            .await?;
            push_pending_chunk(&pending_chunks, chunk)?;
        }
    }
    drop(live_tx);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_live_processor(
    app: AppHandle,
    repo: Repository,
    project_id: String,
    provider_id: String,
    key: Zeroizing<Vec<u8>>,
    provider: Arc<dyn Provider>,
    translation: Option<String>,
    analysis: String,
    context: ProviderContext,
    accept_results: Arc<AtomicBool>,
    remote_deferred: Arc<AtomicBool>,
    mut rx: mpsc::Receiver<LiveWork>,
) {
    while let Some(work) = rx.recv().await {
        if context.cancelled.load(Ordering::Acquire) || !accept_results.load(Ordering::Acquire) {
            break;
        }
        match work {
            LiveWork::Partial {
                track_role,
                start_ms,
                sample_rate,
                samples,
            } => {
                if remote_deferred.load(Ordering::Acquire) {
                    continue;
                }
                let path = match write_wav(
                    &repo,
                    &project_id,
                    track_role,
                    sample_rate,
                    &samples,
                    "partial-live",
                )
                .await
                {
                    Ok(path) => path,
                    Err(_) => continue,
                };
                let temporary = TemporaryPath::from_existing(path);
                let result = tokio::time::timeout(
                    LIVE_TRANSCRIPTION_TIMEOUT,
                    provider.transcribe_realtime_chunk(
                        temporary.path(),
                        start_ms,
                        track_role,
                        &context,
                    ),
                )
                .await;
                match result {
                    Ok(Ok(drafts)) if accept_results.load(Ordering::Acquire) => {
                        if let Some(draft) = drafts.last() {
                            app.emit("accordmesh://timeline-partial",json!({
                                "projectId":project_id,"trackRole":track_role,"startMs":draft.start_ms,
                                "endMs":draft.end_ms,"sourceTranscript":draft.text,"transcriptStatus":"partial"
                            })).ok();
                        }
                    }
                    Ok(Err("ERR_JOB_CANCELLED")) => break,
                    Ok(Err(_)) | Err(_) => {
                        remote_deferred.store(true, Ordering::Release);
                        app.emit(
                            "accordmesh://realtime-deferred",
                            json!({"projectId":project_id}),
                        )
                        .ok();
                    }
                    _ => {}
                }
            }
            LiveWork::Final(chunk) => {
                if remote_deferred.load(Ordering::Acquire) {
                    continue;
                }
                let result = process_pending_chunk_guarded(
                    Some(&app),
                    &repo,
                    &project_id,
                    &provider_id,
                    &key,
                    provider.as_ref(),
                    &chunk,
                    translation.as_deref(),
                    &analysis,
                    &context,
                    Some(accept_results.as_ref()),
                )
                .await;
                match result {
                    Ok(()) => {}
                    Err("ERR_JOB_CANCELLED") => break,
                    Err(_) => {
                        remote_deferred.store(true, Ordering::Release);
                        app.emit(
                            "accordmesh://realtime-deferred",
                            json!({"projectId":project_id}),
                        )
                        .ok();
                    }
                }
            }
        }
    }
}

fn push_pending_chunk(
    pending_chunks: &Arc<Mutex<Vec<PendingRealtimeChunk>>>,
    chunk: PendingRealtimeChunk,
) -> Result<(), &'static str> {
    pending_chunks.lock().map_err(|_| "ERR_STATE")?.push(chunk);
    Ok(())
}

fn spool_session_path(repo: &Repository, project_id: &str) -> PathBuf {
    repo.realtime_pending_dir(project_id).join("session.json")
}
fn spool_chunk_metadata_path(repo: &Repository, project_id: &str, chunk_id: &str) -> PathBuf {
    repo.realtime_pending_dir(project_id)
        .join(format!("{chunk_id}.json"))
}
fn spool_chunk_transcript_path(repo: &Repository, project_id: &str, chunk_id: &str) -> PathBuf {
    repo.realtime_pending_dir(project_id)
        .join(format!("{chunk_id}.transcript.json.enc"))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    let parent = path.parent().ok_or("ERR_IO")?;
    std::fs::create_dir_all(parent).map_err(|_| "ERR_IO")?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("spool"),
        Uuid::new_v4()
    ));
    if let Err(_) = std::fs::write(&temporary, bytes) {
        let _ = std::fs::remove_file(&temporary);
        return Err("ERR_IO");
    }
    if path.exists() {
        std::fs::remove_file(path).map_err(|_| "ERR_IO")?;
    }
    std::fs::rename(&temporary, path).map_err(|_| {
        let _ = std::fs::remove_file(&temporary);
        "ERR_IO"
    })
}

pub(crate) fn write_spool_session(
    repo: &Repository,
    session: &RealtimeSpoolSession,
) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(session).map_err(|_| "ERR_JSON")?;
    atomic_write(&spool_session_path(repo, &session.project_id), &bytes)
}

pub(crate) fn read_spool_session(
    repo: &Repository,
    project_id: &str,
) -> Result<Option<RealtimeSpoolSession>, &'static str> {
    let path = spool_session_path(repo, project_id);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|_| "ERR_IO")?;
    let session: RealtimeSpoolSession =
        serde_json::from_slice(&bytes).map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
    if session.project_id != project_id || session.format_version != REALTIME_SPOOL_VERSION {
        return Err("ERR_JOB_PAYLOAD");
    }
    Ok(Some(session))
}

pub(crate) fn finalization_payload_from_session(
    session: &RealtimeSpoolSession,
) -> serde_json::Value {
    json!({
        "realtimeFinalize":true,"realtimeSpoolVersion":session.format_version,
        "providerId":session.provider_id,"sourceLanguage":session.source_language,
        "translationLanguage":session.translation_language,"outputLanguage":session.output_language,
        "pendingRealtimeChunks":[]
    })
}

pub(crate) fn recover_spool_jobs(repo: &Repository) -> Result<(), &'static str> {
    for project in repo.list_projects().map_err(|_| "ERR_STORAGE")? {
        let Some(session) = read_spool_session(repo, &project.id)? else {
            continue;
        };
        if matches!(project.status, ProjectStatus::Completed) {
            complete_realtime_spool(repo, &project.id)?;
            continue;
        }
        if repo
            .has_realtime_finalization_job(&project.id)
            .map_err(|_| "ERR_STORAGE")?
        {
            continue;
        }
        let payload = finalization_payload_from_session(&session);
        repo.queue_job(&project.id, None, "realtime_finalize", 30, &payload)
            .map_err(|_| "ERR_STORAGE")?;
        repo.set_project_status(&project.id, ProjectStatus::Processing)
            .map_err(|_| "ERR_STORAGE")?;
    }
    Ok(())
}

fn encode_spool_envelope(
    header: &RealtimeSpoolEnvelopeHeader,
    wav_bytes: &[u8],
) -> Result<Zeroizing<Vec<u8>>, &'static str> {
    let metadata = serde_json::to_vec(header).map_err(|_| "ERR_JSON")?;
    let metadata_len = u32::try_from(metadata.len()).map_err(|_| "ERR_MEDIA_SIZE")?;
    let mut encoded = Zeroizing::new(Vec::with_capacity(10 + metadata.len() + wav_bytes.len()));
    encoded.extend_from_slice(REALTIME_SPOOL_MAGIC);
    encoded.extend_from_slice(&header.format_version.to_be_bytes());
    encoded.extend_from_slice(&metadata_len.to_be_bytes());
    encoded.extend_from_slice(&metadata);
    encoded.extend_from_slice(wav_bytes);
    Ok(encoded)
}

fn decode_spool_envelope(
    bytes: &[u8],
) -> Result<(RealtimeSpoolEnvelopeHeader, &[u8]), &'static str> {
    if bytes.len() < 10 || &bytes[..4] != REALTIME_SPOOL_MAGIC {
        return Err("ERR_ENCRYPTED_DATA_CORRUPT");
    }
    let outer_version = u16::from_be_bytes([bytes[4], bytes[5]]);
    let metadata_len = u32::from_be_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]) as usize;
    let metadata_end = 10usize
        .checked_add(metadata_len)
        .ok_or("ERR_ENCRYPTED_DATA_CORRUPT")?;
    if metadata_end > bytes.len() {
        return Err("ERR_ENCRYPTED_DATA_CORRUPT");
    }
    let header: RealtimeSpoolEnvelopeHeader = serde_json::from_slice(&bytes[10..metadata_end])
        .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
    if header.format_version != outer_version {
        return Err("ERR_ENCRYPTED_DATA_CORRUPT");
    }
    Ok((header, &bytes[metadata_end..]))
}

pub(crate) fn load_pending_chunks(
    repo: &Repository,
    project_id: &str,
    project_key: &[u8],
) -> Result<Vec<PendingRealtimeChunk>, &'static str> {
    let directory = repo.realtime_pending_dir(project_id);
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut chunks = Vec::new();
    for entry in std::fs::read_dir(&directory).map_err(|_| "ERR_IO")? {
        let path = entry.map_err(|_| "ERR_IO")?.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if name == "session.json" || name.ends_with(".transcript.json.enc") {
            continue;
        }
        if name.ends_with(".chunk.enc") {
            let encoded = std::fs::read(&path).map_err(|_| "ERR_IO")?;
            let sealed = crypto::from_slice(&encoded).map_err(|_| "ERR_CRYPTO")?;
            let plaintext =
                Zeroizing::new(crypto::open(project_key, &sealed).map_err(|_| "ERR_CRYPTO")?);
            let (envelope, wav_bytes) = decode_spool_envelope(&plaintext)?;
            let expected_name = format!("{}.chunk.enc", envelope.id);
            if envelope.format_version != REALTIME_SPOOL_VERSION
                || envelope.id.is_empty()
                || wav_bytes.is_empty()
                || !path.starts_with(&directory)
                || name != expected_name
            {
                return Err("ERR_JOB_PAYLOAD");
            }
            chunks.push(PendingRealtimeChunk {
                format_version: envelope.format_version,
                id: envelope.id,
                track_role: envelope.track_role,
                start_ms: envelope.start_ms,
                end_ms: envelope.end_ms,
                sample_rate: envelope.sample_rate,
                encrypted_path: path.to_string_lossy().into_owned(),
            });
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|_| "ERR_IO")?;
        let chunk: PendingRealtimeChunk =
            serde_json::from_slice(&bytes).map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
        let encrypted = PathBuf::from(&chunk.encrypted_path);
        let expected_name = format!("{}.wav.enc", chunk.id);
        if chunk.format_version != 1
            || chunk.id.is_empty()
            || !encrypted.starts_with(&directory)
            || encrypted.file_name().and_then(|value| value.to_str())
                != Some(expected_name.as_str())
        {
            return Err("ERR_JOB_PAYLOAD");
        }
        if encrypted.exists() {
            chunks.push(chunk);
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
    chunks.sort_by(|left, right| (left.start_ms, &left.id).cmp(&(right.start_ms, &right.id)));
    Ok(chunks)
}

pub(crate) fn merge_pending_chunks(
    primary: Vec<PendingRealtimeChunk>,
    legacy: Vec<PendingRealtimeChunk>,
) -> Vec<PendingRealtimeChunk> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for chunk in primary.into_iter().chain(legacy) {
        if seen.insert(chunk.id.clone()) {
            merged.push(chunk);
        }
    }
    merged.sort_by(|left, right| (left.start_ms, &left.id).cmp(&(right.start_ms, &right.id)));
    merged
}

pub(crate) fn complete_realtime_spool(
    repo: &Repository,
    project_id: &str,
) -> Result<(), &'static str> {
    let directory = repo.realtime_pending_dir(project_id);
    if directory.exists() {
        std::fs::remove_dir_all(directory).map_err(|_| "ERR_IO")?;
    }
    Ok(())
}

async fn stage_pending_wav(
    repo: &Repository,
    project_id: &str,
    project_key: &[u8],
    plaintext_path: &Path,
    start_ms: i64,
    track_role: TrackRole,
    sample_rate: u32,
) -> Result<PendingRealtimeChunk, &'static str> {
    let id = Uuid::new_v4().to_string();
    let plaintext = match tokio::fs::read(plaintext_path).await {
        Ok(value) => Zeroizing::new(value),
        Err(_) => {
            tokio::fs::remove_file(plaintext_path).await.ok();
            return Err("ERR_IO");
        }
    };
    let duration_ms =
        plaintext.len().saturating_sub(44) as i64 / 2 * 1000 / sample_rate.max(1) as i64;
    let end_ms = start_ms.saturating_add(duration_ms);
    let envelope = RealtimeSpoolEnvelopeHeader {
        format_version: REALTIME_SPOOL_VERSION,
        id: id.clone(),
        track_role,
        start_ms,
        end_ms,
        sample_rate,
    };
    let serialized = match encode_spool_envelope(&envelope, plaintext.as_slice()) {
        Ok(value) => value,
        Err(code) => {
            tokio::fs::remove_file(plaintext_path).await.ok();
            return Err(code);
        }
    };
    let sealed = match crypto::seal(project_key, &serialized) {
        Ok(value) => value,
        Err(_) => {
            tokio::fs::remove_file(plaintext_path).await.ok();
            return Err("ERR_CRYPTO");
        }
    };
    let encoded = match crypto::to_vec(&sealed) {
        Ok(value) => value,
        Err(_) => {
            tokio::fs::remove_file(plaintext_path).await.ok();
            return Err("ERR_CRYPTO");
        }
    };
    let directory = repo.realtime_pending_dir(project_id);
    let encrypted_path = directory.join(format!("{id}.chunk.enc"));
    if let Err(code) = atomic_write(&encrypted_path, &encoded) {
        tokio::fs::remove_file(plaintext_path).await.ok();
        return Err(code);
    }
    if tokio::fs::remove_file(plaintext_path).await.is_err() {
        return Err("ERR_IO");
    }
    Ok(PendingRealtimeChunk {
        format_version: REALTIME_SPOOL_VERSION,
        id,
        track_role,
        start_ms,
        end_ms,
        sample_rate,
        encrypted_path: encrypted_path.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
pub(crate) async fn stage_pending_wav_for_test(
    repo: &Repository,
    project_id: &str,
    project_key: &[u8],
    plaintext_path: &Path,
    start_ms: i64,
    track_role: TrackRole,
    sample_rate: u32,
) -> Result<PendingRealtimeChunk, &'static str> {
    stage_pending_wav(
        repo,
        project_id,
        project_key,
        plaintext_path,
        start_ms,
        track_role,
        sample_rate,
    )
    .await
}

fn commit_allowed(flag: Option<&AtomicBool>) -> Result<(), &'static str> {
    if flag
        .map(|value| value.load(Ordering::Acquire))
        .unwrap_or(true)
    {
        Ok(())
    } else {
        Err("ERR_JOB_CANCELLED")
    }
}

fn load_cached_transcript(
    repo: &Repository,
    project_id: &str,
    project_key: &[u8],
    chunk_id: &str,
) -> Result<Option<Vec<TranscriptDraft>>, &'static str> {
    let path = spool_chunk_transcript_path(repo, project_id, chunk_id);
    if !path.exists() {
        return Ok(None);
    }
    let encoded = std::fs::read(path).map_err(|_| "ERR_IO")?;
    let sealed = crypto::from_slice(&encoded).map_err(|_| "ERR_CRYPTO")?;
    let plaintext = crypto::open(project_key, &sealed).map_err(|_| "ERR_CRYPTO")?;
    serde_json::from_slice(&plaintext)
        .map(Some)
        .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")
}

fn store_cached_transcript(
    repo: &Repository,
    project_id: &str,
    project_key: &[u8],
    chunk_id: &str,
    drafts: &[TranscriptDraft],
) -> Result<(), &'static str> {
    let plaintext = serde_json::to_vec(drafts).map_err(|_| "ERR_JSON")?;
    let sealed = crypto::seal(project_key, &plaintext).map_err(|_| "ERR_CRYPTO")?;
    let encoded = crypto::to_vec(&sealed).map_err(|_| "ERR_CRYPTO")?;
    atomic_write(
        &spool_chunk_transcript_path(repo, project_id, chunk_id),
        &encoded,
    )
}

fn remove_spool_chunk(repo: &Repository, project_id: &str, chunk: &PendingRealtimeChunk) {
    let _ = std::fs::remove_file(&chunk.encrypted_path);
    let _ = std::fs::remove_file(spool_chunk_transcript_path(repo, project_id, &chunk.id));
    let _ = std::fs::remove_file(spool_chunk_metadata_path(repo, project_id, &chunk.id));
}

async fn load_spooled_wav(
    repo: &Repository,
    project_id: &str,
    project_key: &[u8],
    chunk: &PendingRealtimeChunk,
) -> Result<Zeroizing<Vec<u8>>, &'static str> {
    let encrypted_path = PathBuf::from(&chunk.encrypted_path);
    if !encrypted_path.starts_with(repo.realtime_pending_dir(project_id)) {
        return Err("ERR_JOB_PAYLOAD");
    }
    let encoded = tokio::fs::read(&encrypted_path)
        .await
        .map_err(|_| "ERR_IO")?;
    let sealed = crypto::from_slice(&encoded).map_err(|_| "ERR_CRYPTO")?;
    let plaintext = Zeroizing::new(crypto::open(project_key, &sealed).map_err(|_| "ERR_CRYPTO")?);
    if chunk.format_version < REALTIME_SPOOL_VERSION {
        return Ok(plaintext);
    }
    let (envelope, wav_bytes) = decode_spool_envelope(&plaintext)?;
    if envelope.format_version != REALTIME_SPOOL_VERSION
        || envelope.id != chunk.id
        || envelope.track_role != chunk.track_role
        || envelope.start_ms != chunk.start_ms
        || envelope.end_ms != chunk.end_ms
        || envelope.sample_rate != chunk.sample_rate
        || wav_bytes.is_empty()
    {
        return Err("ERR_JOB_PAYLOAD");
    }
    Ok(Zeroizing::new(wav_bytes.to_vec()))
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn process_pending_chunk(
    app: Option<&AppHandle>,
    repo: &Repository,
    project_id: &str,
    provider_id: &str,
    project_key: &[u8],
    provider: &dyn Provider,
    chunk: &PendingRealtimeChunk,
    translation: Option<&str>,
    analysis: &str,
    context: &ProviderContext,
) -> Result<(), &'static str> {
    process_pending_chunk_guarded(
        app,
        repo,
        project_id,
        provider_id,
        project_key,
        provider,
        chunk,
        translation,
        analysis,
        context,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn process_pending_chunk_guarded(
    app: Option<&AppHandle>,
    repo: &Repository,
    project_id: &str,
    provider_id: &str,
    project_key: &[u8],
    provider: &dyn Provider,
    chunk: &PendingRealtimeChunk,
    translation: Option<&str>,
    analysis: &str,
    context: &ProviderContext,
    accept_results: Option<&AtomicBool>,
) -> Result<(), &'static str> {
    let encrypted_path = PathBuf::from(&chunk.encrypted_path);
    if !encrypted_path.starts_with(repo.realtime_pending_dir(project_id)) {
        return Err("ERR_JOB_PAYLOAD");
    }
    let drafts = match load_cached_transcript(repo, project_id, project_key, &chunk.id)? {
        Some(value) => value,
        None => {
            let plaintext = load_spooled_wav(repo, project_id, project_key, chunk).await?;
            let temp_path = repo.data_dir().join("temp").join(format!(
                "realtime-spool-{}-{}.wav",
                chunk.id,
                Uuid::new_v4()
            ));
            if tokio::fs::write(&temp_path, plaintext.as_slice())
                .await
                .is_err()
            {
                return Err("ERR_IO");
            }
            let temporary = TemporaryPath::from_existing(temp_path);
            let drafts = provider
                .transcribe_realtime_chunk(
                    temporary.path(),
                    chunk.start_ms,
                    chunk.track_role,
                    context,
                )
                .await?;
            commit_allowed(accept_results)?;
            store_cached_transcript(repo, project_id, project_key, &chunk.id, &drafts)?;
            drafts
        }
    };
    commit_allowed(accept_results)?;
    for (index, draft) in drafts.into_iter().enumerate() {
        let segment_id = format!("realtime-{}-{index:04}", chunk.id);
        let source_id = format!("realtime-chunk-{}", chunk.id);
        persist_final_with_identity(
            app,
            repo,
            project_id,
            provider_id,
            project_key,
            provider,
            draft,
            chunk.track_role,
            translation,
            analysis,
            context,
            Some(segment_id),
            Some(source_id),
            Some(chunk.id.as_str()),
        )
        .await?;
        commit_allowed(accept_results)?;
    }
    commit_allowed(accept_results)?;
    remove_spool_chunk(repo, project_id, chunk);
    Ok(())
}

async fn receive_audio_frame(
    rx: &mut mpsc::Receiver<AudioFrame>,
    runtime_errors: &mut mpsc::UnboundedReceiver<&'static str>,
    stopping: &AtomicBool,
) -> Result<Option<AudioFrame>, &'static str> {
    tokio::select! {
        Some(code)=runtime_errors.recv()=>{
            if stopping.load(Ordering::Relaxed){Ok(None)}else{Err(code)}
        }
        frame=rx.recv()=>match frame{
            Some(frame)=>Ok(Some(frame)),
            None if stopping.load(Ordering::Relaxed)=>Ok(None),
            None=>Err("ERR_AUDIO_RUNTIME"),
        }
    }
}

fn consume_analyze_generation(last_analyze_generation: &mut u64, generation: u64) -> bool {
    let force = generation > *last_analyze_generation;
    if force {
        *last_analyze_generation = generation;
    }
    force
}

async fn transcribe_final_chunk(
    provider: &dyn Provider,
    path: &Path,
    offset_ms: i64,
    track: TrackRole,
    context: &ProviderContext,
) -> Result<Vec<TranscriptDraft>, &'static str> {
    let result = provider
        .transcribe_realtime_chunk(path, offset_ms, track, context)
        .await;
    tokio::fs::remove_file(path).await.ok();
    result
}

#[cfg(test)]
pub(crate) async fn receive_audio_frame_for_test(
    rx: &mut mpsc::Receiver<AudioFrame>,
    microphone_errors: &mut mpsc::UnboundedReceiver<&'static str>,
    stopping: &AtomicBool,
) -> Result<Option<AudioFrame>, &'static str> {
    receive_audio_frame(rx, microphone_errors, stopping).await
}

#[cfg(test)]
pub(crate) fn consume_analyze_generation_for_test(
    last_analyze_generation: &mut u64,
    generation: u64,
) -> bool {
    consume_analyze_generation(last_analyze_generation, generation)
}

#[cfg(test)]
pub(crate) async fn transcribe_final_chunk_for_test(
    provider: &dyn Provider,
    path: &Path,
    offset_ms: i64,
    track: TrackRole,
    context: &ProviderContext,
) -> Result<Vec<TranscriptDraft>, &'static str> {
    transcribe_final_chunk(provider, path, offset_ms, track, context).await
}

async fn persist_final(
    app: &AppHandle,
    repo: &Repository,
    project_id: &str,
    provider_id: &str,
    key: &[u8],
    provider: &dyn Provider,
    draft: TranscriptDraft,
    track: TrackRole,
    translation: Option<&str>,
    analysis: &str,
    context: &ProviderContext,
) -> Result<(), &'static str> {
    persist_final_with_identity(
        Some(app),
        repo,
        project_id,
        provider_id,
        key,
        provider,
        draft,
        track,
        translation,
        analysis,
        context,
        None,
        None,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn persist_final_with_identity(
    app: Option<&AppHandle>,
    repo: &Repository,
    project_id: &str,
    provider_id: &str,
    key: &[u8],
    provider: &dyn Provider,
    draft: TranscriptDraft,
    track: TrackRole,
    translation: Option<&str>,
    analysis: &str,
    context: &ProviderContext,
    segment_id: Option<String>,
    source_id: Option<String>,
    generation_scope: Option<&str>,
) -> Result<(), &'static str> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let segment = TimelineSegment {
        id: segment_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        project_id: project_id.into(),
        source_id: source_id.unwrap_or_else(|| format!("realtime-{project_id}")),
        track_role: track,
        start_ms: draft.start_ms,
        end_ms: draft.end_ms.max(draft.start_ms),
        source_transcript: draft.text,
        detected_language: draft.detected_language,
        transcript_status: "final".into(),
        confidence: draft.confidence,
        warnings: vec![],
        created_at: Utc::now().to_rfc3339(),
    };
    repo.insert_segment(&segment, key)
        .map_err(|_| "ERR_STORAGE")?;
    if let Some(app) = app {
        app.emit(
            "accordmesh://timeline-final",
            json!({"sequence":sequence,"segment":segment}),
        )
        .ok();
    }
    let translated_text = if let Some(target_language) = translation {
        let translation_input = GenerationInput {
            project_id: project_id.into(),
            source_ids: vec![segment.id.clone()],
            source_text: segment.source_transcript.clone(),
            output_language: target_language.into(),
            context_json: json!({"trackRole":track,"startMs":segment.start_ms,"endMs":segment.end_ms}),
        };
        let translated = if let Some(scope) = generation_scope {
            persist_realtime_chunk_generation(
                repo,
                scope,
                project_id,
                provider_id,
                key,
                "literal_translation",
                provider.model_id_for("text_translation"),
                provider.translate(&translation_input, context),
                translation_input.source_ids.clone(),
            )
            .await?
        } else {
            persist_generation(
                repo,
                project_id,
                provider_id,
                key,
                "literal_translation",
                provider.model_id_for("text_translation"),
                provider.translate(&translation_input, context),
                translation_input.source_ids.clone(),
            )
            .await?
        };
        translated.payload.get("translatedText").cloned()
    } else {
        None
    };
    let understanding = if let Some(scope) = generation_scope {
        persist_realtime_chunk_generation(
            repo,
            scope,
            project_id,
            provider_id,
            key,
            "segment_understanding",
            provider.model_id_for("segment_understanding"),
            provider.understand_segment(&segment, analysis, context),
            vec![segment.id.clone()],
        )
        .await?
    } else {
        persist_generation(
            repo,
            project_id,
            provider_id,
            key,
            "segment_understanding",
            provider.model_id_for("segment_understanding"),
            provider.understand_segment(&segment, analysis, context),
            vec![segment.id.clone()],
        )
        .await?
    };
    if let Some(app) = app {
        app.emit(
            "accordmesh://realtime-understanding",
            json!({
                "projectId":project_id,"sequence":sequence,"segment":segment,
                "translation":translated_text,"payload":understanding.payload
            }),
        )
        .ok();
    }
    Ok(())
}

async fn write_wav(
    repo: &Repository,
    project_id: &str,
    track: TrackRole,
    sample_rate: u32,
    samples: &[i16],
    label: &str,
) -> Result<PathBuf, &'static str> {
    let path = repo.data_dir().join("temp").join(format!(
        "{project_id}-{track:?}-{label}-{}.wav",
        Uuid::new_v4()
    ));
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    tokio::fs::write(&path, bytes).await.map_err(|_| "ERR_IO")?;
    Ok(path)
}
