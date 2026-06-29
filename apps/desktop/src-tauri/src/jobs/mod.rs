use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::analysis::validate_artifact;
use crate::media;
use crate::projects::types::*;
use crate::providers::{self, GeneratedDraft, GenerationInput, ProviderContext, TranscriptDraft};
use crate::realtime;
use crate::storage::repository::Repository;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedFile {
    #[serde(default)]
    pub queued_file_id: String,
    #[serde(default)]
    pub temp_path: Option<PathBuf>,
    #[serde(default)]
    pub asset_id: Option<String>,
    pub original_file_name: String,
    pub kind: MediaKind,
    pub sha256: String,
    pub size: u64,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadJobPayload {
    pub files: Vec<QueuedFile>,
    pub provider_id: String,
    pub source_language: Option<String>,
    pub translation_target_language: Option<String>,
    pub analysis_output_language: String,
    pub minutes_output_language: String,
    pub attach_to_existing: bool,
}

#[derive(Clone)]
pub struct JobRuntime {
    pub cancellation: Arc<AtomicBool>,
    completion: Arc<(Mutex<bool>, Condvar)>,
}

pub type RuntimeRegistry = Arc<Mutex<HashMap<String, JobRuntime>>>;

pub fn runtime_registry() -> RuntimeRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

pub fn runtime_exists(registry: &RuntimeRegistry, job_id: &str) -> bool {
    registry
        .lock()
        .map(|items| items.contains_key(job_id))
        .unwrap_or(true)
}

pub fn active_runtime_count(registry: &RuntimeRegistry) -> usize {
    registry
        .lock()
        .map(|items| items.len())
        .unwrap_or(usize::MAX)
}

pub fn has_active_runtimes(registry: &RuntimeRegistry) -> bool {
    active_runtime_count(registry) > 0
}

pub fn request_cancel(registry: &RuntimeRegistry, job_id: &str) {
    if let Ok(items) = registry.lock() {
        if let Some(runtime) = items.get(job_id) {
            runtime.cancellation.store(true, Ordering::Relaxed);
        }
    }
}

pub fn cancel_all_and_wait(registry: &RuntimeRegistry, timeout: Duration) -> bool {
    let runtimes = match registry.lock() {
        Ok(items) => items.values().cloned().collect::<Vec<_>>(),
        Err(_) => return false,
    };
    for runtime in &runtimes {
        runtime.cancellation.store(true, Ordering::Relaxed);
    }
    let deadline = Instant::now() + timeout;
    for runtime in runtimes {
        let (lock, signal) = &*runtime.completion;
        let mut completed = match lock.lock() {
            Ok(value) => value,
            Err(_) => return false,
        };
        while !*completed {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            match signal.wait_timeout(completed, remaining) {
                Ok((value, result)) => {
                    completed = value;
                    if result.timed_out() && !*completed {
                        return false;
                    }
                }
                Err(_) => return false,
            }
        }
    }
    true
}

fn register_runtime(registry: &RuntimeRegistry, job_id: &str) -> Result<JobRuntime, &'static str> {
    let runtime = JobRuntime {
        cancellation: Arc::new(AtomicBool::new(false)),
        completion: Arc::new((Mutex::new(false), Condvar::new())),
    };
    let mut items = registry.lock().map_err(|_| "ERR_STATE")?;
    if items.contains_key(job_id) {
        return Err("ERR_JOB_ALREADY_RUNNING");
    }
    items.insert(job_id.to_owned(), runtime.clone());
    Ok(runtime)
}

fn complete_runtime(registry: &RuntimeRegistry, job_id: &str, runtime: &JobRuntime) {
    if let Ok(mut items) = registry.lock() {
        items.remove(job_id);
    }
    let (lock, signal) = &*runtime.completion;
    if let Ok(mut completed) = lock.lock() {
        *completed = true;
        signal.notify_all();
    }
}

#[cfg(test)]
pub(crate) fn register_test_runtime(
    registry: &RuntimeRegistry,
    job_id: &str,
) -> Result<JobRuntime, &'static str> {
    register_runtime(registry, job_id)
}

#[cfg(test)]
pub(crate) fn complete_test_runtime(
    registry: &RuntimeRegistry,
    job_id: &str,
    runtime: &JobRuntime,
) {
    complete_runtime(registry, job_id, runtime);
}

pub fn spawn_upload(
    app: AppHandle,
    repo: Repository,
    master_key: Zeroizing<Vec<u8>>,
    job_id: String,
    registry: RuntimeRegistry,
) -> Result<(), &'static str> {
    let runtime = register_runtime(&registry, &job_id)?;
    tauri::async_runtime::spawn(async move {
        let result = run_upload(
            Some(&app),
            &repo,
            &master_key,
            &job_id,
            &runtime.cancellation,
        )
        .await;
        if let Err(code) = result {
            finalize_upload_failure(Some(&app), &repo, &job_id, code);
        }
        drop(master_key);
        complete_runtime(&registry, &job_id, &runtime);
    });
    Ok(())
}

pub fn spawn_regeneration(
    app: AppHandle,
    repo: Repository,
    master_key: Zeroizing<Vec<u8>>,
    job_id: String,
    registry: RuntimeRegistry,
) -> Result<(), &'static str> {
    let runtime = register_runtime(&registry, &job_id)?;
    tauri::async_runtime::spawn(async move {
        let result = run_regeneration(
            Some(&app),
            &repo,
            &master_key,
            &job_id,
            &runtime.cancellation,
        )
        .await;
        if let Err(code) = result {
            let status = if code == "ERR_JOB_CANCELLED" {
                "cancelled"
            } else {
                "failed"
            };
            repo.update_job(&job_id, status, status, 0.0, Some(code))
                .ok();
            app.emit(
                "accordmesh://job-error",
                json!({"jobId":job_id,"errorCode":code}),
            )
            .ok();
        }
        drop(master_key);
        complete_runtime(&registry, &job_id, &runtime);
    });
    Ok(())
}

pub fn spawn_recovered(
    app: AppHandle,
    repo: Repository,
    master_key: Zeroizing<Vec<u8>>,
    job_id: String,
    registry: RuntimeRegistry,
) -> Result<(), &'static str> {
    let payload = repo
        .job_payload(&job_id)
        .ok()
        .map(|(_, payload)| payload)
        .unwrap_or_default();
    if payload
        .get("realtimeFinalize")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        spawn_realtime_finalization(app, repo, master_key, job_id, registry)
    } else if payload.get("artifactType").is_some() {
        spawn_regeneration(app, repo, master_key, job_id, registry)
    } else {
        spawn_upload(app, repo, master_key, job_id, registry)
    }
}

pub fn spawn_realtime_finalization(
    app: AppHandle,
    repo: Repository,
    master_key: Zeroizing<Vec<u8>>,
    job_id: String,
    registry: RuntimeRegistry,
) -> Result<(), &'static str> {
    let runtime = register_runtime(&registry, &job_id)?;
    tauri::async_runtime::spawn(async move {
        let result =
            run_realtime_finalization(&app, &repo, &master_key, &job_id, &runtime.cancellation)
                .await;
        if let Err(code) = result {
            let status = if code == "ERR_JOB_CANCELLED" {
                "cancelled"
            } else {
                "failed"
            };
            repo.update_job(&job_id, status, status, 0.0, Some(code))
                .ok();
            if let Ok((project_id, _)) = repo.job_payload(&job_id) {
                repo.set_project_status(&project_id, ProjectStatus::Failed)
                    .ok();
            }
            app.emit(
                "accordmesh://job-error",
                json!({"jobId":job_id,"errorCode":code}),
            )
            .ok();
        }
        drop(master_key);
        complete_runtime(&registry, &job_id, &runtime);
    });
    Ok(())
}

trait FinalizationEventSink: Sync {
    fn progress(&self, job_id: &str, status: &str, stage: &str, progress: f64, error: Option<&str>);
    fn project_completed(&self, project_id: &str);
}

impl FinalizationEventSink for AppHandle {
    fn progress(
        &self,
        job_id: &str,
        status: &str,
        stage: &str,
        progress: f64,
        error: Option<&str>,
    ) {
        self.emit("accordmesh://job-progress",json!({"jobId":job_id,"status":status,"stage":stage,"progress":progress,"errorCode":error})).ok();
    }

    fn project_completed(&self, project_id: &str) {
        self.emit(
            "accordmesh://project-status",
            json!({"projectId":project_id,"status":"completed"}),
        )
        .ok();
    }
}

#[cfg(test)]
struct SilentFinalizationEvents;

#[cfg(test)]
impl FinalizationEventSink for SilentFinalizationEvents {
    fn progress(
        &self,
        _job_id: &str,
        _status: &str,
        _stage: &str,
        _progress: f64,
        _error: Option<&str>,
    ) {
    }
    fn project_completed(&self, _project_id: &str) {}
}

fn update_finalization(
    events: &dyn FinalizationEventSink,
    repo: &Repository,
    job_id: &str,
    status: &str,
    stage: &str,
    progress: f64,
    error: Option<&str>,
) -> Result<(), &'static str> {
    repo.update_job(job_id, status, stage, progress, error)
        .map_err(|_| "ERR_STORAGE")?;
    events.progress(job_id, status, stage, progress, error);
    Ok(())
}

async fn run_realtime_finalization(
    app: &AppHandle,
    repo: &Repository,
    master_key: &[u8],
    job_id: &str,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), &'static str> {
    let (_, payload) = repo.job_payload(job_id).map_err(|_| "ERR_STORAGE")?;
    let provider_id = payload
        .get("providerId")
        .and_then(|value| value.as_str())
        .ok_or("ERR_JOB_PAYLOAD")?;
    let provider = providers::registry::resolve(provider_id, repo, master_key)?;
    run_realtime_finalization_pipeline(app, repo, master_key, job_id, cancelled, provider).await
}

async fn run_realtime_finalization_pipeline(
    events: &dyn FinalizationEventSink,
    repo: &Repository,
    master_key: &[u8],
    job_id: &str,
    cancelled: &Arc<AtomicBool>,
    provider: Arc<dyn providers::Provider>,
) -> Result<(), &'static str> {
    let (project_id, mut payload) = repo.job_payload(job_id).map_err(|_| "ERR_STORAGE")?;
    let provider_id = payload
        .get("providerId")
        .and_then(|value| value.as_str())
        .ok_or("ERR_JOB_PAYLOAD")?
        .to_owned();
    let language = payload
        .get("outputLanguage")
        .and_then(|value| value.as_str())
        .unwrap_or("en")
        .to_owned();
    let source_language = payload
        .get("sourceLanguage")
        .and_then(|value| value.as_str())
        .map(str::to_owned);
    let translation_language = payload
        .get("translationLanguage")
        .and_then(|value| value.as_str())
        .map(str::to_owned);
    let legacy_chunks: Vec<realtime::PendingRealtimeChunk> = serde_json::from_value(
        payload
            .get("pendingRealtimeChunks")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(|_| "ERR_JOB_PAYLOAD")?;
    let project_key = repo
        .project_key(&project_id, master_key)
        .map_err(|_| "ERR_CRYPTO")?;
    let mut pending_chunks = realtime::merge_pending_chunks(
        realtime::load_pending_chunks(repo, &project_id, &project_key)?,
        legacy_chunks,
    );

    if !pending_chunks.is_empty() {
        let mut required = vec!["realtime_transcription", "segment_understanding"];
        if translation_language.is_some() {
            required.push("text_translation");
        }
        providers::registry::require(&provider.capabilities(), &required)?;
        update_finalization(events, repo, job_id, "running", "transcribing", 0.05, None)?;
        while let Some(chunk) = pending_chunks.first().cloned() {
            ensure_active(repo, job_id, cancelled)?;
            let context = ProviderContext {
                cancelled: cancelled.clone(),
                source_language: source_language.clone(),
                model_override: None,
            };
            realtime::process_pending_chunk(
                None,
                repo,
                &project_id,
                &provider_id,
                &project_key,
                provider.as_ref(),
                &chunk,
                translation_language.as_deref(),
                &language,
                &context,
            )
            .await?;
            pending_chunks.remove(0);
            payload["pendingRealtimeChunks"] = json!(pending_chunks.clone());
            repo.update_job_payload(job_id, &payload)
                .map_err(|_| "ERR_STORAGE")?;
        }
    }

    ensure_active(repo, job_id, cancelled)?;
    let detail = repo
        .project_detail(&project_id, master_key)
        .map_err(|_| "ERR_STORAGE")?;
    if detail.timeline.is_empty() {
        repo.set_project_status(&project_id, ProjectStatus::Completed)
            .map_err(|_| "ERR_STORAGE")?;
        realtime::complete_realtime_spool(repo, &project_id)?;
        update_finalization(events, repo, job_id, "completed", "completed", 1.0, None)?;
        events.project_completed(&project_id);
        return Ok(());
    }

    providers::registry::require(
        &provider.capabilities(),
        &[
            "meeting_synthesis",
            "communication_review",
            "meeting_minutes",
        ],
    )?;
    update_finalization(events, repo, job_id, "running", "analyzing", 0.2, None)?;
    let source_ids = detail
        .timeline
        .iter()
        .map(|segment| segment.id.clone())
        .collect::<Vec<_>>();
    let common = GenerationInput {
        project_id: project_id.clone(),
        source_ids: source_ids.clone(),
        source_text: transcript_text(&detail.timeline),
        output_language: language.clone(),
        context_json: evidence_context(&detail.timeline),
    };
    let context = ProviderContext {
        cancelled: cancelled.clone(),
        source_language: None,
        model_override: None,
    };
    let analysis = persist_realtime_finalization_generation(
        repo,
        job_id,
        &project_id,
        &provider_id,
        &project_key,
        "post_meeting_analysis",
        provider.model_id_for("meeting_synthesis"),
        provider.synthesize_meeting(&common, &context),
        source_ids.clone(),
    )
    .await?;
    ensure_active(repo, job_id, cancelled)?;
    update_finalization(events, repo, job_id, "running", "reviewing", 0.6, None)?;
    let review = persist_realtime_finalization_generation(
        repo,
        job_id,
        &project_id,
        &provider_id,
        &project_key,
        "communication_review",
        provider.model_id_for("communication_review"),
        provider.review_communication(&common, &context),
        source_ids.clone(),
    )
    .await?;
    ensure_active(repo, job_id, cancelled)?;
    update_finalization(events, repo, job_id, "running", "minutes", 0.82, None)?;
    let mut minutes_context = common.context_json.clone();
    minutes_context["sourceArtifactIds"] =
        json!([analysis.artifact_id.clone(), review.artifact_id.clone()]);
    minutes_context["sourceArtifacts"] = json!([
        {"id":analysis.artifact_id.clone(),"artifactType":"post_meeting_analysis","sourceIds":source_ids.clone(),"payload":analysis.payload.clone()},
        {"id":review.artifact_id.clone(),"artifactType":"communication_review","sourceIds":source_ids.clone(),"payload":review.payload.clone()}
    ]);
    let minutes_input = GenerationInput {
        context_json: minutes_context,
        ..common
    };
    persist_realtime_finalization_generation(
        repo,
        job_id,
        &project_id,
        &provider_id,
        &project_key,
        "meeting_minutes",
        provider.model_id_for("meeting_minutes"),
        provider.minutes(&minutes_input, &context),
        minutes_input.source_ids.clone(),
    )
    .await?;
    repo.set_project_status(&project_id, ProjectStatus::Completed)
        .map_err(|_| "ERR_STORAGE")?;
    realtime::complete_realtime_spool(repo, &project_id)?;
    update_finalization(events, repo, job_id, "completed", "completed", 1.0, None)?;
    events.project_completed(&project_id);
    Ok(())
}

#[cfg(test)]
pub(crate) async fn run_realtime_finalization_for_test(
    repo: &Repository,
    master_key: &[u8],
    job_id: &str,
    cancelled: &Arc<AtomicBool>,
    provider: Option<Arc<dyn providers::Provider>>,
) -> Result<(), &'static str> {
    let resolved = match provider {
        Some(value) => value,
        None => {
            let (_, payload) = repo.job_payload(job_id).map_err(|_| "ERR_STORAGE")?;
            let provider_id = payload
                .get("providerId")
                .and_then(|value| value.as_str())
                .ok_or("ERR_JOB_PAYLOAD")?;
            providers::registry::resolve(provider_id, repo, master_key)?
        }
    };
    run_realtime_finalization_pipeline(
        &SilentFinalizationEvents,
        repo,
        master_key,
        job_id,
        cancelled,
        resolved,
    )
    .await
}

async fn run_regeneration(
    app: Option<&AppHandle>,
    repo: &Repository,
    master_key: &[u8],
    job_id: &str,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), &'static str> {
    let (project_id, payload) = repo.job_payload(job_id).map_err(|_| "ERR_STORAGE")?;
    let artifact_type = payload
        .get("artifactType")
        .and_then(|value| value.as_str())
        .ok_or("ERR_JOB_PAYLOAD")?;
    let provider_id = payload
        .get("providerId")
        .and_then(|value| value.as_str())
        .ok_or("ERR_JOB_PAYLOAD")?;
    let language = payload
        .get("outputLanguage")
        .and_then(|value| value.as_str())
        .unwrap_or("en");
    let requested_model = payload
        .get("modelId")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let requested_segments = payload
        .get("sourceSegmentIds")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let requested_artifacts = payload
        .get("sourceArtifactIds")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let provider = providers::registry::resolve(provider_id, repo, master_key)?;
    let capability = match artifact_type {
        "literal_translation" => "text_translation",
        "segment_understanding" => "segment_understanding",
        "post_meeting_analysis" => "meeting_synthesis",
        "communication_review" => "communication_review",
        "intelligent_comparison_report" => "comparison_report",
        "meeting_minutes" => "meeting_minutes",
        _ => return Err("ERR_ARTIFACT_NOT_REGENERATABLE"),
    };
    providers::registry::require(&provider.capabilities(), &[capability])?;
    let default_model = provider.model_id_for(capability);
    let selected_model =
        validate_regeneration_model(provider_id, &default_model, requested_model.as_deref())?;
    ensure_active(repo, job_id, cancelled)?;
    update_optional(app, repo, job_id, "running", "generating", 0.1, None)?;
    let detail = repo
        .project_detail(&project_id, master_key)
        .map_err(|_| "ERR_STORAGE")?;
    if requested_segments.is_empty() {
        return Err("ERR_JOB_PAYLOAD");
    }
    if artifact_type == "segment_understanding" && requested_segments.len() != 1 {
        return Err("ERR_JOB_PAYLOAD");
    }
    let selected = detail
        .timeline
        .iter()
        .filter(|segment| requested_segments.contains(&segment.id))
        .cloned()
        .collect::<Vec<_>>();
    if selected.len() != requested_segments.len() {
        return Err("ERR_JOB_PAYLOAD");
    }

    let unique_artifact_ids = requested_artifacts.iter().collect::<HashSet<_>>();
    if unique_artifact_ids.len() != requested_artifacts.len() {
        return Err("ERR_JOB_PAYLOAD");
    }
    let selected_artifacts = requested_artifacts
        .iter()
        .map(|artifact_id| {
            detail
                .artifacts
                .iter()
                .find(|artifact| artifact.id == *artifact_id)
                .cloned()
                .ok_or("ERR_JOB_PAYLOAD")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if artifact_type == "meeting_minutes" {
        if selected_artifacts.len() != 2 {
            return Err("ERR_JOB_PAYLOAD");
        }
        let analysis_count = selected_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_type == "post_meeting_analysis")
            .count();
        let review_count = selected_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_type == "communication_review")
            .count();
        if analysis_count != 1 || review_count != 1 {
            return Err("ERR_JOB_PAYLOAD");
        }
    }

    let source_ids = selected
        .iter()
        .map(|segment| segment.id.clone())
        .collect::<Vec<_>>();
    let source_artifacts = selected_artifacts
        .iter()
        .map(|artifact| {
            json!({
                "id":artifact.id.clone(),
                "artifactType":artifact.artifact_type.clone(),
                "sourceIds":artifact.source_ids.clone(),
                "schemaVersion":artifact.schema_version.clone(),
                "promptVersion":artifact.prompt_version.clone(),
                "providerId":artifact.provider_id.clone(),
                "modelId":artifact.model_id.clone(),
                "createdAt":artifact.created_at.clone(),
                "payload":artifact.payload.clone(),
            })
        })
        .collect::<Vec<_>>();
    let mut context_json = evidence_context(&selected);
    if !selected_artifacts.is_empty() {
        context_json["sourceArtifactIds"] = json!(requested_artifacts.clone());
        context_json["sourceArtifacts"] = json!(source_artifacts.clone());
    }
    if artifact_type == "intelligent_comparison_report" {
        let realtime = selected
            .iter()
            .filter(|segment| segment.track_role != TrackRole::UploadedMedia)
            .cloned()
            .collect::<Vec<_>>();
        let uploaded = selected
            .iter()
            .filter(|segment| segment.track_role == TrackRole::UploadedMedia)
            .cloned()
            .collect::<Vec<_>>();
        if realtime.is_empty() || uploaded.is_empty() {
            return Err("ERR_JOB_PAYLOAD");
        }
        context_json = json!({"realtimeEvidence":evidence_context(&realtime),"uploadedEvidence":evidence_context(&uploaded)});
        if !selected_artifacts.is_empty() {
            context_json["sourceArtifactIds"] = json!(requested_artifacts.clone());
            context_json["sourceArtifacts"] = json!(source_artifacts);
        }
    }
    let source_text = if artifact_type == "literal_translation" {
        translation_source_text(&selected)
    } else {
        transcript_text(&selected)
    };
    if source_text.trim().is_empty() {
        return Err("ERR_TRANSCRIPT_EMPTY");
    }
    let input = GenerationInput {
        project_id: project_id.clone(),
        source_ids: source_ids.clone(),
        source_text,
        output_language: language.into(),
        context_json,
    };
    let context = ProviderContext {
        cancelled: cancelled.clone(),
        source_language: None,
        model_override: if selected_model == default_model {
            None
        } else {
            Some(selected_model.clone())
        },
    };
    let request: std::pin::Pin<
        Box<dyn Future<Output = Result<GeneratedDraft, &'static str>> + Send + '_>,
    > = match artifact_type {
        "literal_translation" => Box::pin(provider.translate(&input, &context)),
        "segment_understanding" => {
            let segment = selected.first().ok_or("ERR_TRANSCRIPT_EMPTY")?;
            Box::pin(provider.understand_segment(segment, language, &context))
        }
        "post_meeting_analysis" => Box::pin(provider.synthesize_meeting(&input, &context)),
        "communication_review" => Box::pin(provider.review_communication(&input, &context)),
        "intelligent_comparison_report" => Box::pin(provider.compare(&input, &context)),
        "meeting_minutes" => Box::pin(provider.minutes(&input, &context)),
        _ => unreachable!(),
    };
    let key = repo
        .project_key(&project_id, master_key)
        .map_err(|_| "ERR_CRYPTO")?;
    persist_generation(
        repo,
        &project_id,
        provider_id,
        &key,
        artifact_type,
        selected_model,
        request,
        source_ids,
    )
    .await?;
    update_optional(app, repo, job_id, "completed", "completed", 1.0, None)?;
    emit_optional(
        app,
        "accordmesh://project-status",
        json!({"projectId":project_id,"status":"completed"}),
    );
    Ok(())
}

pub(crate) fn validate_regeneration_model(
    provider_id: &str,
    default_model: &str,
    requested_model: Option<&str>,
) -> Result<String, &'static str> {
    let selected = requested_model.unwrap_or(default_model).trim();
    if selected.is_empty()
        || selected.len() > 160
        || !selected.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':' | '/')
        })
    {
        return Err("ERR_PROVIDER_MODEL_INVALID");
    }
    if provider_id != "openai" && selected != default_model {
        return Err("ERR_PROVIDER_MODEL_UNSUPPORTED");
    }
    Ok(selected.to_owned())
}

#[cfg(test)]
pub(crate) async fn execute_regeneration_for_test(
    repo: &Repository,
    master_key: &[u8],
    job_id: &str,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), &'static str> {
    let result = run_regeneration(None, repo, master_key, job_id, cancelled).await;
    if let Err(code) = result {
        let status = if code == "ERR_JOB_CANCELLED" {
            "cancelled"
        } else {
            "failed"
        };
        repo.update_job(job_id, status, status, 0.0, Some(code))
            .ok();
    }
    result
}

async fn run_upload(
    app: Option<&AppHandle>,
    repo: &Repository,
    master_key: &[u8],
    job_id: &str,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), &'static str> {
    let (project_id, value) = repo.job_payload(job_id).map_err(|_| "ERR_STORAGE")?;
    let mut payload: UploadJobPayload =
        serde_json::from_value(value).map_err(|_| "ERR_JOB_PAYLOAD")?;
    let provider = providers::registry::resolve(&payload.provider_id, repo, master_key)?;
    let needs_transcription = payload
        .files
        .iter()
        .any(|f| matches!(f.kind, MediaKind::Audio | MediaKind::Video));
    let mut capabilities = vec![
        "segment_understanding",
        "meeting_synthesis",
        "communication_review",
        "meeting_minutes",
    ];
    if payload.translation_target_language.is_some() {
        capabilities.push("text_translation");
    }
    if needs_transcription {
        capabilities.push("file_transcription");
    }
    if payload.attach_to_existing {
        capabilities.push("comparison_report");
    }
    let provider_capabilities = provider.capabilities();
    providers::registry::require(&provider_capabilities, &capabilities)?;
    providers::registry::validate_language_contract(
        &provider_capabilities,
        payload.source_language.as_deref(),
        payload.translation_target_language.as_deref(),
        &[
            payload.analysis_output_language.as_str(),
            payload.minutes_output_language.as_str(),
        ],
    )?;
    let context = ProviderContext {
        cancelled: cancelled.clone(),
        source_language: payload.source_language.clone(),
        model_override: None,
    };
    let project_key = repo
        .project_key(&project_id, master_key)
        .map_err(|_| "ERR_CRYPTO")?;
    update_optional(app, repo, job_id, "running", "importing", 0.03, None)?;
    let existing = if payload.attach_to_existing {
        repo.project_detail(&project_id, master_key)
            .map_err(|_| "ERR_STORAGE")?
            .timeline
    } else {
        Vec::new()
    };
    let mut imported_segments = Vec::new();
    for index in 0..payload.files.len() {
        ensure_active(repo, job_id, cancelled)?;
        let legacy_asset = if payload.files[index].queued_file_id.is_empty() {
            repo.media_for_job_legacy(job_id, &payload.files[index].original_file_name)
                .map_err(|_| "ERR_STORAGE")?
        } else {
            None
        };
        if let Some(asset) = legacy_asset.as_ref() {
            payload.files[index].queued_file_id = asset.id.clone();
            payload.files[index].asset_id = Some(asset.id.clone());
            payload.files[index].temp_path = None;
            repo.update_job_payload(
                job_id,
                &serde_json::to_value(&payload).map_err(|_| "ERR_JSON")?,
            )
            .map_err(|_| "ERR_STORAGE")?;
        } else if payload.files[index].queued_file_id.is_empty() {
            payload.files[index].queued_file_id = Uuid::new_v4().to_string();
            repo.update_job_payload(
                job_id,
                &serde_json::to_value(&payload).map_err(|_| "ERR_JSON")?,
            )
            .map_err(|_| "ERR_STORAGE")?;
        }
        let file = payload.files[index].clone();
        let asset = if let Some(asset) = legacy_asset.or(repo
            .media_for_job(job_id, &file.queued_file_id)
            .map_err(|_| "ERR_STORAGE")?)
        {
            asset
        } else {
            let source = file.temp_path.clone().ok_or("ERR_MEDIA_SOURCE_MISSING")?;
            let source_guard = media::TemporaryPath::from_existing(source);
            let asset = repo
                .import_media_asset(
                    &project_id,
                    job_id,
                    &file.queued_file_id,
                    &file.original_file_name,
                    file.kind,
                    file.mime_type.clone(),
                    source_guard.path(),
                    &project_key,
                )
                .await?;
            if asset.sha256 != file.sha256 {
                repo.delete_media_asset(&asset.id).ok();
                return Err("ERR_MEDIA_CHANGED");
            }
            payload.files[index].asset_id = Some(asset.id.clone());
            payload.files[index].temp_path = None;
            repo.update_job_payload(
                job_id,
                &serde_json::to_value(&payload).map_err(|_| "ERR_JSON")?,
            )
            .map_err(|_| "ERR_STORAGE")?;
            asset
        };
        if payload.files[index].asset_id.is_none() {
            payload.files[index].asset_id = Some(asset.id.clone());
            payload.files[index].temp_path = None;
            repo.update_job_payload(
                job_id,
                &serde_json::to_value(&payload).map_err(|_| "ERR_JSON")?,
            )
            .map_err(|_| "ERR_STORAGE")?;
        }
        let materialized = repo
            .materialize_media_asset(&asset.id, &project_key)
            .await?;
        let drafts = match file.kind {
            MediaKind::Transcript | MediaKind::Subtitle => {
                if file.size > media::MAX_TEXT_BYTES {
                    return Err("ERR_MEDIA_SIZE");
                }
                let bytes = tokio::fs::read(materialized.path())
                    .await
                    .map_err(|_| "ERR_MEDIA_READ")?;
                media::parse_text(file.kind, &bytes)?
            }
            MediaKind::Audio | MediaKind::Video => {
                update_optional(
                    app,
                    repo,
                    job_id,
                    "running",
                    "preparing_media",
                    0.12 + (index as f64 / payload.files.len().max(1) as f64) * 0.12,
                    None,
                )?;
                let work = media::TemporaryDirectory::create(
                    &repo.data_dir().join("temp"),
                    &format!("job-{job_id}"),
                )?;
                let (chunks, duration) = media::prepare_media_cancellable(
                    materialized.path(),
                    file.kind,
                    work.path(),
                    cancelled,
                )
                .await?;
                repo.update_media_status(&asset.id, "transcribing", duration)
                    .map_err(|_| "ERR_STORAGE")?;
                let mut drafts = Vec::new();
                for (chunk_index, chunk) in chunks.iter().enumerate() {
                    ensure_active(repo, job_id, cancelled)?;
                    let hash = media::sha256_file(&chunk.path).await?;
                    let stored = repo
                        .ensure_media_chunk(
                            &asset.id,
                            chunk_index as i64,
                            chunk.start_ms,
                            chunk.end_ms,
                            chunk.overlap_ms,
                            &hash,
                        )
                        .map_err(|error| match error {
                            crate::storage::repository::EnsureMediaChunkError::Contract => {
                                "ERR_MEDIA_CHANGED"
                            }
                            crate::storage::repository::EnsureMediaChunkError::Sql(_) => {
                                "ERR_STORAGE"
                            }
                        })?;
                    let result = if stored.state == "completed" {
                        serde_json::from_value(
                            repo.load_chunk_transcript(&stored, &project_key)
                                .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?,
                        )
                        .map_err(|_| "ERR_JSON")?
                    } else {
                        repo.mark_chunk_running(&stored.id)
                            .map_err(|_| "ERR_STORAGE")?;
                        let extension = chunk
                            .path
                            .extension()
                            .and_then(|value| value.to_str())
                            .unwrap_or("wav");
                        let provider_name = format!(
                            "{}-chunk-{}.{}",
                            Path::new(&file.original_file_name)
                                .file_stem()
                                .and_then(|value| value.to_str())
                                .unwrap_or("meeting"),
                            chunk_index,
                            extension
                        );
                        let mime = mime_guess::from_path(&chunk.path).first_raw();
                        let input = providers::TranscriptionInput {
                            path: &chunk.path,
                            original_file_name: &provider_name,
                            mime_type: mime,
                            offset_ms: chunk.start_ms,
                            end_ms: Some(chunk.end_ms),
                            track_role: TrackRole::UploadedMedia,
                            chunk_index: chunk_index as i64,
                        };
                        match provider.transcribe_file(&input, &context).await {
                            Ok(result) => {
                                repo.store_chunk_transcript(
                                    &stored.id,
                                    &project_id,
                                    &serde_json::to_value(&result).map_err(|_| "ERR_JSON")?,
                                    &project_key,
                                )
                                .map_err(|_| "ERR_STORAGE")?;
                                result
                            }
                            Err(code) => {
                                repo.update_chunk_result(&stored.id, "failed", Some(code))
                                    .ok();
                                return Err(code);
                            }
                        }
                    };
                    merge_drafts(&mut drafts, result, chunk.overlap_ms);
                    update_optional(
                        app,
                        repo,
                        job_id,
                        "running",
                        "transcribing",
                        0.25 + (chunk_index as f64 + 1.0) / chunks.len().max(1) as f64 * 0.25,
                        None,
                    )?;
                }
                drafts
            }
        };
        let source_id = asset.id.clone();
        let mut prior = repo
            .segments_for_source(&project_id, &source_id, &project_key)
            .map_err(|_| "ERR_STORAGE")?;
        for draft in drafts {
            if prior.iter().any(|segment| {
                segment.start_ms == draft.start_ms
                    && segment.end_ms == draft.end_ms.max(draft.start_ms)
            }) {
                continue;
            }
            let segment = segment_from_draft(&project_id, &source_id, draft);
            repo.insert_segment(&segment, &project_key)
                .map_err(|_| "ERR_STORAGE")?;
            emit_optional(app, "accordmesh://timeline-final", &segment);
            prior.push(segment);
        }
        prior.sort_by_key(|segment| (segment.start_ms, segment.end_ms));
        imported_segments.extend(prior);
        repo.update_media_status(&asset.id, "ready", None)
            .map_err(|_| "ERR_STORAGE")?;
    }
    if imported_segments.is_empty() {
        return Err("ERR_TRANSCRIPT_EMPTY");
    }
    update_optional(app, repo, job_id, "running", "translating", 0.56, None)?;
    for segment in &imported_segments {
        ensure_active(repo, job_id, cancelled)?;
        if let Some(target_language) = payload.translation_target_language.as_deref() {
            let input = GenerationInput {
                project_id: project_id.clone(),
                source_ids: vec![segment.id.clone()],
                source_text: segment.source_transcript.clone(),
                output_language: target_language.to_string(),
                context_json: evidence_context(std::slice::from_ref(segment)),
            };
            persist_upload_generation(
                repo,
                job_id,
                &project_id,
                &payload.provider_id,
                &project_key,
                "literal_translation",
                provider.model_id_for("text_translation"),
                provider.translate(&input, &context),
                input.source_ids.clone(),
            )
            .await?;
            ensure_active(repo, job_id, cancelled)?;
        }
        persist_upload_generation(
            repo,
            job_id,
            &project_id,
            &payload.provider_id,
            &project_key,
            "segment_understanding",
            provider.model_id_for("segment_understanding"),
            provider.understand_segment(segment, &payload.analysis_output_language, &context),
            vec![segment.id.clone()],
        )
        .await?;
        ensure_active(repo, job_id, cancelled)?;
    }
    let source_ids = imported_segments
        .iter()
        .map(|s| s.id.clone())
        .collect::<Vec<_>>();
    let source_text = transcript_text(&imported_segments);
    let common = GenerationInput {
        project_id: project_id.clone(),
        source_ids: source_ids.clone(),
        source_text: source_text.clone(),
        output_language: payload.analysis_output_language.clone(),
        context_json: evidence_context(&imported_segments),
    };
    update_optional(app, repo, job_id, "running", "analyzing", 0.68, None)?;
    let analysis_artifact_id = persist_upload_generation(
        repo,
        job_id,
        &project_id,
        &payload.provider_id,
        &project_key,
        "post_meeting_analysis",
        provider.model_id_for("meeting_synthesis"),
        provider.synthesize_meeting(&common, &context),
        source_ids.clone(),
    )
    .await?
    .artifact_id;
    ensure_active(repo, job_id, cancelled)?;
    let review_artifact_id = persist_upload_generation(
        repo,
        job_id,
        &project_id,
        &payload.provider_id,
        &project_key,
        "communication_review",
        provider.model_id_for("communication_review"),
        provider.review_communication(&common, &context),
        source_ids.clone(),
    )
    .await?
    .artifact_id;
    ensure_active(repo, job_id, cancelled)?;
    if payload.attach_to_existing {
        update_optional(app, repo, job_id, "running", "comparing", 0.82, None)?;
        let realtime_segments = existing
            .iter()
            .filter(|segment| segment.track_role != TrackRole::UploadedMedia)
            .cloned()
            .collect::<Vec<_>>();
        let mut seen_uploaded_ids = HashSet::new();
        let uploaded_segments = existing
            .iter()
            .filter(|segment| segment.track_role == TrackRole::UploadedMedia)
            .chain(imported_segments.iter())
            .filter(|segment| seen_uploaded_ids.insert(segment.id.clone()))
            .cloned()
            .collect::<Vec<_>>();
        let comparison = GenerationInput {
            project_id: project_id.clone(),
            source_ids: realtime_segments
                .iter()
                .chain(uploaded_segments.iter())
                .map(|segment| segment.id.clone())
                .collect(),
            source_text: format!(
                "REAL-TIME:\n{}\n\nUPLOADED:\n{}",
                transcript_text(&realtime_segments),
                transcript_text(&uploaded_segments)
            ),
            output_language: payload.analysis_output_language.clone(),
            context_json: json!({"realtimeEvidence":evidence_context(&realtime_segments),"uploadedEvidence":evidence_context(&uploaded_segments)}),
        };
        let sources = comparison.source_ids.clone();
        persist_upload_generation(
            repo,
            job_id,
            &project_id,
            &payload.provider_id,
            &project_key,
            "intelligent_comparison_report",
            provider.model_id_for("comparison_report"),
            provider.compare(&comparison, &context),
            sources,
        )
        .await?;
        ensure_active(repo, job_id, cancelled)?;
    }
    update_optional(app, repo, job_id, "running", "minutes", 0.9, None)?;
    let mut minutes_context = common.context_json.clone();
    minutes_context["sourceArtifactIds"] = json!([analysis_artifact_id, review_artifact_id]);
    let minutes_input = GenerationInput {
        output_language: payload.minutes_output_language,
        context_json: minutes_context,
        ..common
    };
    let sources = minutes_input.source_ids.clone();
    persist_upload_generation(
        repo,
        job_id,
        &project_id,
        &payload.provider_id,
        &project_key,
        "meeting_minutes",
        provider.model_id_for("meeting_minutes"),
        provider.minutes(&minutes_input, &context),
        sources,
    )
    .await?;
    ensure_active(repo, job_id, cancelled)?;
    repo.set_project_status(&project_id, ProjectStatus::Completed)
        .map_err(|_| "ERR_STORAGE")?;
    update_optional(app, repo, job_id, "completed", "completed", 1.0, None)?;
    emit_optional(
        app,
        "accordmesh://project-status",
        json!({"projectId":project_id,"status":"completed"}),
    );
    Ok(())
}

fn finalize_upload_failure(
    app: Option<&AppHandle>,
    repo: &Repository,
    job_id: &str,
    code: &'static str,
) {
    let payload = repo.job_payload(job_id).ok();
    let status = if code == "ERR_JOB_CANCELLED" {
        "cancelled"
    } else {
        "failed"
    };
    repo.update_job(job_id, status, status, 0.0, Some(code))
        .ok();
    if let Some((project_id, value)) = payload {
        let attachment = serde_json::from_value::<UploadJobPayload>(value)
            .map(|payload| payload.attach_to_existing)
            .unwrap_or(false);
        repo.set_project_status(
            &project_id,
            if attachment {
                ProjectStatus::Completed
            } else {
                ProjectStatus::Failed
            },
        )
        .ok();
    }
    emit_optional(
        app,
        "accordmesh://job-error",
        json!({"jobId":job_id,"errorCode":code}),
    );
}

#[cfg(test)]
pub(crate) async fn execute_upload_for_test(
    repo: &Repository,
    master_key: &[u8],
    job_id: &str,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), &'static str> {
    let result = run_upload(None, repo, master_key, job_id, cancelled).await;
    if let Err(code) = result {
        finalize_upload_failure(None, repo, job_id, code);
    }
    result
}

pub(crate) struct PersistedGeneration {
    pub artifact_id: String,
    pub payload: serde_json::Value,
}

async fn persist_realtime_finalization_generation<F>(
    repo: &Repository,
    job_id: &str,
    project_id: &str,
    provider_id: &str,
    project_key: &[u8],
    expected_type: &str,
    model_id: String,
    request: F,
    source_ids: Vec<String>,
) -> Result<PersistedGeneration, &'static str>
where
    F: Future<Output = Result<GeneratedDraft, &'static str>> + Send,
{
    let identity = realtime_finalization_generation_identity(job_id, expected_type, &source_ids);
    let run_id = format!("realtime-finalize-run-{identity}");
    let artifact_id = format!("realtime-finalize-artifact-{identity}");
    if let Some(artifact) = repo
        .completed_artifact_by_id(project_id, &artifact_id, project_key)
        .map_err(|_| "ERR_STORAGE")?
    {
        if artifact.project_id != project_id
            || artifact.artifact_type != expected_type
            || artifact.provider_id != provider_id
            || artifact.source_ids.as_slice() != source_ids.as_slice()
        {
            return Err("ERR_STORAGE");
        }
        return Ok(PersistedGeneration {
            artifact_id: artifact.id,
            payload: artifact.payload,
        });
    }
    if repo
        .generation_run_status(&run_id)
        .map_err(|_| "ERR_STORAGE")?
        .as_deref()
        == Some("completed")
    {
        return Err("ERR_STORAGE");
    }
    let version = artifact_version(expected_type);
    repo.begin_or_restart_generation_run(
        &run_id,
        project_id,
        provider_id,
        &model_id,
        version,
        version,
        &source_ids,
    )
    .map_err(|_| "ERR_STORAGE")?;
    let draft = match request.await {
        Ok(value) => value,
        Err(code) => {
            repo.fail_generation(&run_id, code).ok();
            return Err(code);
        }
    };
    if draft.artifact_type != expected_type {
        repo.fail_generation(&run_id, "ERR_PROVIDER_SCHEMA").ok();
        return Err("ERR_PROVIDER_SCHEMA");
    }
    if let Err(code) = validate_artifact(draft.artifact_type, &draft.payload) {
        repo.fail_generation(&run_id, code).ok();
        return Err(code);
    }
    let artifact = AnalysisArtifact {
        id: artifact_id,
        project_id: project_id.into(),
        artifact_type: draft.artifact_type.into(),
        source_ids,
        schema_version: draft.schema_version.into(),
        prompt_version: draft.prompt_version.into(),
        provider_id: provider_id.into(),
        model_id: draft.model_id,
        app_version: env!("CARGO_PKG_VERSION").into(),
        created_at: Utc::now().to_rfc3339(),
        status: "completed".into(),
        payload: draft.payload,
    };
    let artifact_id = artifact.id.clone();
    let payload = artifact.payload.clone();
    if repo
        .complete_generation(&run_id, &artifact, project_key)
        .is_err()
    {
        repo.fail_generation(&run_id, "ERR_STORAGE").ok();
        return Err("ERR_STORAGE");
    }
    Ok(PersistedGeneration {
        artifact_id,
        payload,
    })
}

fn realtime_finalization_generation_identity(
    job_id: &str,
    artifact_type: &str,
    source_ids: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"accordmesh-realtime-finalization-v1\0");
    for value in [job_id, artifact_type] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    for source_id in source_ids {
        hasher.update((source_id.len() as u64).to_be_bytes());
        hasher.update(source_id.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

pub(crate) async fn persist_realtime_chunk_generation<F>(
    repo: &Repository,
    chunk_id: &str,
    project_id: &str,
    provider_id: &str,
    project_key: &[u8],
    expected_type: &str,
    model_id: String,
    request: F,
    source_ids: Vec<String>,
) -> Result<PersistedGeneration, &'static str>
where
    F: Future<Output = Result<GeneratedDraft, &'static str>> + Send,
{
    let identity = realtime_chunk_generation_identity(chunk_id, expected_type, &source_ids);
    let run_id = format!("realtime-chunk-run-{identity}");
    let artifact_id = format!("realtime-chunk-artifact-{identity}");
    if let Some(artifact) = repo
        .completed_artifact_by_id(project_id, &artifact_id, project_key)
        .map_err(|_| "ERR_STORAGE")?
    {
        if artifact.project_id != project_id
            || artifact.artifact_type != expected_type
            || artifact.provider_id != provider_id
            || artifact.source_ids.as_slice() != source_ids.as_slice()
        {
            return Err("ERR_STORAGE");
        }
        return Ok(PersistedGeneration {
            artifact_id: artifact.id,
            payload: artifact.payload,
        });
    }
    if repo
        .generation_run_status(&run_id)
        .map_err(|_| "ERR_STORAGE")?
        .as_deref()
        == Some("completed")
    {
        return Err("ERR_STORAGE");
    }
    let version = artifact_version(expected_type);
    repo.begin_or_restart_generation_run(
        &run_id,
        project_id,
        provider_id,
        &model_id,
        version,
        version,
        &source_ids,
    )
    .map_err(|_| "ERR_STORAGE")?;
    let draft = match request.await {
        Ok(value) => value,
        Err(code) => {
            repo.fail_generation(&run_id, code).ok();
            return Err(code);
        }
    };
    if draft.artifact_type != expected_type {
        repo.fail_generation(&run_id, "ERR_PROVIDER_SCHEMA").ok();
        return Err("ERR_PROVIDER_SCHEMA");
    }
    if let Err(code) = validate_artifact(draft.artifact_type, &draft.payload) {
        repo.fail_generation(&run_id, code).ok();
        return Err(code);
    }
    let artifact = AnalysisArtifact {
        id: artifact_id,
        project_id: project_id.into(),
        artifact_type: draft.artifact_type.into(),
        source_ids,
        schema_version: draft.schema_version.into(),
        prompt_version: draft.prompt_version.into(),
        provider_id: provider_id.into(),
        model_id: draft.model_id,
        app_version: env!("CARGO_PKG_VERSION").into(),
        created_at: Utc::now().to_rfc3339(),
        status: "completed".into(),
        payload: draft.payload,
    };
    let artifact_id = artifact.id.clone();
    let payload = artifact.payload.clone();
    if repo
        .complete_generation(&run_id, &artifact, project_key)
        .is_err()
    {
        repo.fail_generation(&run_id, "ERR_STORAGE").ok();
        return Err("ERR_STORAGE");
    }
    Ok(PersistedGeneration {
        artifact_id,
        payload,
    })
}

fn realtime_chunk_generation_identity(
    chunk_id: &str,
    artifact_type: &str,
    source_ids: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"accordmesh-realtime-chunk-v2\0");
    for value in [chunk_id, artifact_type] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    for source_id in source_ids {
        hasher.update((source_id.len() as u64).to_be_bytes());
        hasher.update(source_id.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

async fn persist_upload_generation<F>(
    repo: &Repository,
    job_id: &str,
    project_id: &str,
    provider_id: &str,
    project_key: &[u8],
    expected_type: &str,
    model_id: String,
    request: F,
    source_ids: Vec<String>,
) -> Result<PersistedGeneration, &'static str>
where
    F: Future<Output = Result<GeneratedDraft, &'static str>> + Send,
{
    let identity = upload_generation_identity(job_id, expected_type, &source_ids);
    let run_id = format!("upload-run-{identity}");
    let artifact_id = format!("upload-artifact-{identity}");
    if let Some(artifact) = repo
        .completed_artifact_by_id(project_id, &artifact_id, project_key)
        .map_err(|_| "ERR_STORAGE")?
    {
        if artifact.project_id != project_id
            || artifact.artifact_type != expected_type
            || artifact.provider_id != provider_id
            || artifact.source_ids.as_slice() != source_ids.as_slice()
        {
            return Err("ERR_STORAGE");
        }
        return Ok(PersistedGeneration {
            artifact_id: artifact.id,
            payload: artifact.payload,
        });
    }
    if repo
        .generation_run_status(&run_id)
        .map_err(|_| "ERR_STORAGE")?
        .as_deref()
        == Some("completed")
    {
        return Err("ERR_STORAGE");
    }
    let version = artifact_version(expected_type);
    repo.begin_or_restart_generation_run(
        &run_id,
        project_id,
        provider_id,
        &model_id,
        version,
        version,
        &source_ids,
    )
    .map_err(|_| "ERR_STORAGE")?;
    let draft = match request.await {
        Ok(value) => value,
        Err(code) => {
            repo.fail_generation(&run_id, code).ok();
            return Err(code);
        }
    };
    if draft.artifact_type != expected_type {
        repo.fail_generation(&run_id, "ERR_PROVIDER_SCHEMA").ok();
        return Err("ERR_PROVIDER_SCHEMA");
    }
    if let Err(code) = validate_artifact(draft.artifact_type, &draft.payload) {
        repo.fail_generation(&run_id, code).ok();
        return Err(code);
    }
    let artifact = AnalysisArtifact {
        id: artifact_id,
        project_id: project_id.into(),
        artifact_type: draft.artifact_type.into(),
        source_ids,
        schema_version: draft.schema_version.into(),
        prompt_version: draft.prompt_version.into(),
        provider_id: provider_id.into(),
        model_id: draft.model_id,
        app_version: env!("CARGO_PKG_VERSION").into(),
        created_at: Utc::now().to_rfc3339(),
        status: "completed".into(),
        payload: draft.payload,
    };
    let artifact_id = artifact.id.clone();
    let payload = artifact.payload.clone();
    if repo
        .complete_generation(&run_id, &artifact, project_key)
        .is_err()
    {
        repo.fail_generation(&run_id, "ERR_STORAGE").ok();
        return Err("ERR_STORAGE");
    }
    Ok(PersistedGeneration {
        artifact_id,
        payload,
    })
}

fn upload_generation_identity(job_id: &str, artifact_type: &str, source_ids: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"accordmesh-upload-generation-v1\0");
    for value in [job_id, artifact_type] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    for source_id in source_ids {
        hasher.update((source_id.len() as u64).to_be_bytes());
        hasher.update(source_id.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

pub(crate) async fn persist_generation<F>(
    repo: &Repository,
    project_id: &str,
    provider_id: &str,
    project_key: &[u8],
    expected_type: &str,
    model_id: String,
    request: F,
    source_ids: Vec<String>,
) -> Result<PersistedGeneration, &'static str>
where
    F: Future<Output = Result<GeneratedDraft, &'static str>> + Send,
{
    let version = artifact_version(expected_type);
    let run = repo
        .begin_generation_run(
            project_id,
            provider_id,
            &model_id,
            version,
            version,
            &source_ids,
        )
        .map_err(|_| "ERR_STORAGE")?;
    let draft = match request.await {
        Ok(value) => value,
        Err(code) => {
            repo.fail_generation(&run, code).ok();
            return Err(code);
        }
    };
    if draft.artifact_type != expected_type {
        repo.fail_generation(&run, "ERR_PROVIDER_SCHEMA").ok();
        return Err("ERR_PROVIDER_SCHEMA");
    }
    if let Err(code) = validate_artifact(draft.artifact_type, &draft.payload) {
        repo.fail_generation(&run, code).ok();
        return Err(code);
    }
    let artifact = AnalysisArtifact {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.into(),
        artifact_type: draft.artifact_type.into(),
        source_ids,
        schema_version: draft.schema_version.into(),
        prompt_version: draft.prompt_version.into(),
        provider_id: provider_id.into(),
        model_id: draft.model_id,
        app_version: env!("CARGO_PKG_VERSION").into(),
        created_at: Utc::now().to_rfc3339(),
        status: "completed".into(),
        payload: draft.payload,
    };
    let artifact_id = artifact.id.clone();
    let payload = artifact.payload.clone();
    if repo
        .complete_generation(&run, &artifact, project_key)
        .is_err()
    {
        repo.fail_generation(&run, "ERR_STORAGE").ok();
        return Err("ERR_STORAGE");
    }
    Ok(PersistedGeneration {
        artifact_id,
        payload,
    })
}

fn artifact_version(kind: &str) -> &'static str {
    match kind {
        "literal_translation" => "literal-translation-v1",
        "segment_understanding" => "segment-understanding-v1",
        "post_meeting_analysis" => "post-meeting-analysis-v1",
        "communication_review" => "communication-review-v1",
        "intelligent_comparison_report" => "intelligent-comparison-v1",
        "meeting_minutes" => "meeting-minutes-v1",
        _ => "unknown-v1",
    }
}

fn ensure_active(
    repo: &Repository,
    job_id: &str,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), &'static str> {
    if cancelled.load(Ordering::Relaxed) || repo.job_cancelled(job_id).unwrap_or(true) {
        cancelled.store(true, Ordering::Relaxed);
        Err("ERR_JOB_CANCELLED")
    } else {
        Ok(())
    }
}
fn emit_optional<S: Serialize + Clone>(app: Option<&AppHandle>, event: &str, payload: S) {
    if let Some(app) = app {
        app.emit(event, payload).ok();
    }
}
fn update_optional(
    app: Option<&AppHandle>,
    repo: &Repository,
    job_id: &str,
    status: &str,
    stage: &str,
    progress: f64,
    error: Option<&str>,
) -> Result<(), &'static str> {
    repo.update_job(job_id, status, stage, progress, error)
        .map_err(|_| "ERR_STORAGE")?;
    emit_optional(
        app,
        "accordmesh://job-progress",
        json!({"jobId":job_id,"status":status,"stage":stage,"progress":progress,"errorCode":error}),
    );
    Ok(())
}
fn update(
    app: &AppHandle,
    repo: &Repository,
    job_id: &str,
    status: &str,
    stage: &str,
    progress: f64,
    error: Option<&str>,
) -> Result<(), &'static str> {
    update_optional(Some(app), repo, job_id, status, stage, progress, error)
}
fn segment_from_draft(
    project_id: &str,
    source_id: &str,
    draft: TranscriptDraft,
) -> TimelineSegment {
    TimelineSegment {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.into(),
        source_id: source_id.into(),
        track_role: TrackRole::UploadedMedia,
        start_ms: draft.start_ms,
        end_ms: draft.end_ms.max(draft.start_ms),
        source_transcript: draft.text,
        detected_language: draft.detected_language,
        transcript_status: "final".into(),
        confidence: draft.confidence,
        warnings: vec![],
        created_at: Utc::now().to_rfc3339(),
    }
}
fn merge_drafts(
    existing: &mut Vec<TranscriptDraft>,
    mut incoming: Vec<TranscriptDraft>,
    overlap: i64,
) {
    incoming.sort_by_key(|draft| (draft.start_ms, draft.end_ms));
    for mut draft in incoming {
        draft.text = draft.text.trim().to_owned();
        if draft.text.is_empty() {
            continue;
        }
        if let Some(last) = existing.last() {
            if overlap > 0
                && normalize(&last.text) == normalize(&draft.text)
                && draft.start_ms <= last.end_ms
            {
                continue;
            }
            if draft.end_ms <= last.end_ms {
                continue;
            }
            if draft.start_ms < last.end_ms {
                draft.start_ms = last.end_ms;
            }
        }
        if draft.end_ms <= draft.start_ms {
            continue;
        }
        existing.push(draft);
    }
}
#[cfg(test)]
pub(crate) fn merge_drafts_for_test(
    existing: &mut Vec<TranscriptDraft>,
    incoming: Vec<TranscriptDraft>,
    overlap: i64,
) {
    merge_drafts(existing, incoming, overlap)
}
fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn transcript_text(segments: &[TimelineSegment]) -> String {
    segments
        .iter()
        .map(|s| {
            format!(
                "[{}-{} ms][{:?}] {}",
                s.start_ms, s.end_ms, s.track_role, s.source_transcript
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub(crate) fn translation_source_text(segments: &[TimelineSegment]) -> String {
    segments
        .iter()
        .map(|segment| segment.source_transcript.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
fn evidence_context(segments: &[TimelineSegment]) -> serde_json::Value {
    json!({"evidenceRefs":segments.iter().map(|s|json!({"sourceId":s.source_id,"segmentId":s.id,"startMs":s.start_ms,"endMs":s.end_ms,"evidenceType":"explicit_statement","confidence":"high"})).collect::<Vec<_>>()})
}
