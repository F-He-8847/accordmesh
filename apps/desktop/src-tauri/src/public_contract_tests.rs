use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::crypto;
use crate::delete_guard_for_status;
use crate::ensure_attachment_project_eligible;
use crate::export;
use crate::jobs::{
    complete_test_runtime, has_active_runtimes, register_test_runtime, runtime_registry,
    translation_source_text, validate_regeneration_model,
};
use crate::media::NativeSelection;
use crate::projects::types::{
    AnalysisArtifact, ExportFormat, MediaKind, MeetingProject, ProjectDetail, ProjectOrigin,
    ProjectStatus, SelectedFile, TimelineSegment, TrackRole,
};
use crate::providers::mock::MockProvider;
use crate::providers::registry::{
    full_capabilities, validate_language_contract, SUPPORTED_LANGUAGE_CODES,
};
use crate::providers::Provider;
use crate::storage::repository::Repository;
use crate::{
    normalized_project_title, require_attachment_media, require_single_upload_file_count,
    setup_status_from_state, validate_minutes_source_artifacts, validate_regeneration_request_id,
    AppCoreState, AppError, PROJECT_TITLE_MAX_CHARS,
};

struct ScopedDir(PathBuf);

impl ScopedDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "accordmesh-public-test-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("create isolated public test directory");
        Self(path)
    }
}

impl Drop for ScopedDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn error_code(error: AppError) -> String {
    serde_json::to_value(error)
        .expect("serialize stable application error")
        .as_str()
        .expect("stable application error code")
        .to_owned()
}

fn project(
    origin: ProjectOrigin,
    status: ProjectStatus,
    media_asset_ids: Vec<String>,
    has_comparison: bool,
) -> MeetingProject {
    MeetingProject {
        id: "project-1".into(),
        title: "Fictional attachment eligibility project".into(),
        origin,
        status,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
        realtime_session_id: None,
        media_asset_ids,
        timeline_segment_ids: vec![],
        artifact_ids: vec![],
        generation_run_ids: vec![],
        has_comparison,
        has_minutes: true,
    }
}

fn selection(kind: MediaKind) -> NativeSelection {
    NativeSelection {
        path: PathBuf::from("/tmp/accordmesh-fictional-selection"),
        metadata: SelectedFile {
            selection_token: "selection-1".into(),
            original_file_name: "fictional-recording".into(),
            kind,
            size: 128,
            mime_type: None,
        },
    }
}

fn artifact(id: &str, artifact_type: &str) -> AnalysisArtifact {
    AnalysisArtifact {
        id: id.into(),
        project_id: "project-1".into(),
        artifact_type: artifact_type.into(),
        source_ids: vec!["segment-1".into()],
        schema_version: "schema-v1".into(),
        prompt_version: "prompt-v1".into(),
        provider_id: "mock".into(),
        model_id: "mock-model-v1".into(),
        app_version: "0.1.0-alpha.1".into(),
        created_at: "2026-01-01T00:00:00Z".into(),
        status: "completed".into(),
        payload: json!({}),
    }
}

fn export_detail() -> ProjectDetail {
    serde_json::from_value(json!({
        "project": {
            "id": "project-export",
            "title": "Fictional readable export",
            "origin": "realtime_in_person",
            "status": "completed",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T01:00:00Z",
            "realtimeSessionId": "session-1",
            "mediaAssetIds": [],
            "timelineSegmentIds": ["segment-1"],
            "artifactIds": ["analysis-1"],
            "generationRunIds": [],
            "hasComparison": false,
            "hasMinutes": false
        },
        "timeline": [{
            "id": "segment-1",
            "projectId": "project-export",
            "sourceId": "source-1",
            "trackRole": "room_microphone",
            "startMs": 57,
            "endMs": 20067,
            "sourceTranscript": "Fictional transcript sentence.",
            "detectedLanguage": "en",
            "transcriptStatus": "final",
            "confidence": 0.95,
            "warnings": [],
            "createdAt": "2026-01-01T00:00:01Z"
        }],
        "mediaAssets": [],
        "artifacts": [{
            "id": "analysis-1",
            "projectId": "project-export",
            "artifactType": "post_meeting_analysis",
            "sourceIds": ["segment-1"],
            "schemaVersion": "post-meeting-analysis-v1",
            "promptVersion": "post-meeting-analysis-v1",
            "providerId": "mock",
            "modelId": "mock-analysis-v1",
            "appVersion": "0.1.0-alpha.1",
            "createdAt": "2026-01-01T00:00:02Z",
            "status": "completed",
            "payload": {"overview": "Fictional analysis overview."}
        }],
        "generationRuns": [],
        "jobs": [],
        "realtimeSession": null
    }))
    .expect("construct fictional export detail")
}

#[test]
fn vault_status_and_key_access_fail_closed_while_locking() {
    let mut state = AppCoreState::default();
    state.master_key = Some(Zeroizing::new(vec![17u8; 32]));
    assert!(setup_status_from_state(&state).unlocked);

    state.locking = true;
    assert!(!setup_status_from_state(&state).unlocked);
    assert_eq!(
        error_code(state.key().expect_err("locking denies key access")),
        "ERR_LOCKED"
    );
    assert_eq!(
        error_code(
            state
                .cloned_key()
                .expect_err("locking denies cloned key access")
        ),
        "ERR_LOCKED"
    );
}

#[test]
fn sensitive_memory_and_native_selections_are_cleared_together() {
    let mut state = AppCoreState::default();
    state.master_key = Some(Zeroizing::new(vec![23u8; 32]));
    state.selections.insert(
        "fictional-selection".into(),
        selection(MediaKind::Transcript),
    );

    state.clear_sensitive_memory();
    assert!(state.master_key.is_none());
    assert!(state.selections.is_empty());
    assert_eq!(
        error_code(state.key().expect_err("cleared state remains locked")),
        "ERR_LOCKED"
    );
}

#[test]
fn active_job_registry_is_fail_closed() {
    let registry = runtime_registry();
    assert!(!has_active_runtimes(&registry));
    let runtime = register_test_runtime(&registry, "public-test-job")
        .expect("register synthetic job runtime");
    assert!(has_active_runtimes(&registry));
    complete_test_runtime(&registry, "public-test-job", &runtime);
    assert!(!has_active_runtimes(&registry));
}

#[test]
fn project_rename_preserves_creation_order() {
    let dir = ScopedDir::new("rename-order");
    let repository = Repository::new(dir.0.join("app-data")).expect("create repository");
    repository.initialize().expect("initialize repository");
    let master = crypto::random_key();

    let first = repository
        .create_project(
            "First",
            ProjectOrigin::UploadOnly,
            ProjectStatus::Completed,
            &master,
        )
        .expect("create first project");
    std::thread::sleep(std::time::Duration::from_millis(15));
    let second = repository
        .create_project(
            "Second",
            ProjectOrigin::UploadOnly,
            ProjectStatus::Completed,
            &master,
        )
        .expect("create second project");

    let renamed = repository
        .rename_project(&first.id, "First renamed")
        .expect("rename project");
    assert_eq!(renamed.created_at, first.created_at);
    let projects = repository.list_projects().expect("list projects");
    assert_eq!(projects[0].id, second.id);
    assert_eq!(projects[1].id, first.id);
}

#[test]
fn project_delete_guards_match_lifecycle_state() {
    assert_eq!(
        delete_guard_for_status(ProjectStatus::Active),
        Some("ERR_ACTIVE_SESSION")
    );
    assert_eq!(
        delete_guard_for_status(ProjectStatus::Processing),
        Some("ERR_ACTIVE_JOB")
    );
    assert_eq!(delete_guard_for_status(ProjectStatus::Completed), None);
    assert_eq!(delete_guard_for_status(ProjectStatus::Failed), None);
}

#[test]
fn attachments_require_one_completed_realtime_project_and_one_recording() {
    for origin in [
        ProjectOrigin::RealtimeOnline,
        ProjectOrigin::RealtimeInPerson,
    ] {
        assert!(ensure_attachment_project_eligible(&project(
            origin,
            ProjectStatus::Completed,
            vec![],
            false,
        ))
        .is_ok());
    }

    assert!(matches!(
        ensure_attachment_project_eligible(&project(
            ProjectOrigin::UploadOnly,
            ProjectStatus::Completed,
            vec![],
            false,
        )),
        Err(AppError::Stable("ERR_ATTACHMENT_REALTIME_ONLY"))
    ));
    assert!(matches!(
        ensure_attachment_project_eligible(&project(
            ProjectOrigin::RealtimeOnline,
            ProjectStatus::Processing,
            vec![],
            false,
        )),
        Err(AppError::Stable("ERR_ATTACHMENT_PROJECT_NOT_COMPLETED"))
    ));
    assert!(matches!(
        ensure_attachment_project_eligible(&project(
            ProjectOrigin::RealtimeOnline,
            ProjectStatus::Completed,
            vec!["asset-1".into()],
            false,
        )),
        Err(AppError::Stable("ERR_ATTACHMENT_ALREADY_EXISTS"))
    ));

    assert!(require_attachment_media(&[selection(MediaKind::Audio)]).is_ok());
    assert!(require_attachment_media(&[selection(MediaKind::Video)]).is_ok());
    assert!(matches!(
        require_attachment_media(&[selection(MediaKind::Transcript)]),
        Err(AppError::Stable("ERR_ATTACHMENT_MEDIA_REQUIRED"))
    ));
}

#[test]
fn upload_command_accepts_exactly_one_file() {
    assert!(require_single_upload_file_count(1).is_ok());
    for count in [0usize, 2, 3, 10] {
        assert!(matches!(
            require_single_upload_file_count(count),
            Err(AppError::Stable("ERR_SINGLE_FILE_REQUIRED"))
        ));
    }
}

#[test]
fn language_contract_supports_auto_detection_and_fails_closed() {
    let capabilities = full_capabilities(&["audio"]);
    assert!(SUPPORTED_LANGUAGE_CODES.contains(&"en"));
    assert!(SUPPORTED_LANGUAGE_CODES.contains(&"ja"));
    assert!(SUPPORTED_LANGUAGE_CODES.contains(&"zh-Hans"));
    assert!(SUPPORTED_LANGUAGE_CODES.contains(&"zh-Hant"));
    assert!(capabilities.supports_language_auto_detection);

    assert_eq!(
        validate_language_contract(&capabilities, None, Some("ja"), &["en", "zh-Hant"]),
        Ok(())
    );
    assert_eq!(
        validate_language_contract(&capabilities, Some("not-a-language"), Some("ja"), &["en"]),
        Err("ERR_SOURCE_LANGUAGE_UNSUPPORTED")
    );
    assert_eq!(
        validate_language_contract(&capabilities, None, Some("not-a-language"), &["en"]),
        Err("ERR_TARGET_LANGUAGE_UNSUPPORTED")
    );
}

#[test]
fn regeneration_model_and_request_identifiers_reject_header_injection() {
    assert_eq!(
        validate_regeneration_model("openai", "default-model", Some("safe-model-2026")),
        Ok("safe-model-2026".into())
    );
    assert_eq!(
        validate_regeneration_model(
            "openai",
            "default-model",
            Some("model\nAuthorization: leak"),
        ),
        Err("ERR_PROVIDER_MODEL_INVALID")
    );
    assert_eq!(
        validate_regeneration_request_id("2b5f8027-2332-4e91-86d3-2164b66ae122"),
        Ok(())
    );
    assert_eq!(
        validate_regeneration_request_id("request\nAuthorization: leak"),
        Err("ERR_JOB_PAYLOAD")
    );
}

#[test]
fn meeting_minutes_require_analysis_and_communication_review_sources() {
    let artifacts = vec![
        artifact("analysis-1", "post_meeting_analysis"),
        artifact("review-1", "communication_review"),
        artifact("translation-1", "literal_translation"),
    ];
    assert_eq!(
        validate_minutes_source_artifacts(&["analysis-1".into(), "review-1".into()], &artifacts,),
        Ok(())
    );
    assert_eq!(
        validate_minutes_source_artifacts(
            &["analysis-1".into(), "translation-1".into()],
            &artifacts,
        ),
        Err("ERR_JOB_PAYLOAD")
    );
}

#[test]
fn mock_provider_reports_the_models_used_by_generated_artifacts() {
    let provider = MockProvider::from_configuration(&json!({"scenario": "normal"}));
    let expected = [
        ("text_translation", "mock-translation-v1"),
        ("segment_understanding", "mock-understanding-v1"),
        ("meeting_synthesis", "mock-analysis-v1"),
        ("communication_review", "mock-review-v1"),
        ("comparison_report", "mock-comparison-v1"),
        ("meeting_minutes", "mock-minutes-v1"),
    ];
    for (capability, model_id) in expected {
        assert_eq!(provider.model_id_for(capability), model_id);
    }
}

#[test]
fn translation_source_text_excludes_internal_timing_and_track_prefixes() {
    let segments = vec![
        TimelineSegment {
            id: "segment-1".into(),
            project_id: "project-1".into(),
            source_id: "chunk-1".into(),
            track_role: TrackRole::LocalMicrophone,
            start_ms: 57,
            end_ms: 20067,
            source_transcript: "  First clean fictional sentence.  ".into(),
            detected_language: Some("en".into()),
            transcript_status: "completed".into(),
            confidence: Some(0.98),
            warnings: vec![],
            created_at: "2026-01-01T00:00:00Z".into(),
        },
        TimelineSegment {
            id: "segment-2".into(),
            project_id: "project-1".into(),
            source_id: "chunk-2".into(),
            track_role: TrackRole::RemoteSystemAudio,
            start_ms: 20100,
            end_ms: 25000,
            source_transcript: "Second clean fictional sentence.".into(),
            detected_language: Some("en".into()),
            transcript_status: "completed".into(),
            confidence: Some(0.97),
            warnings: vec![],
            created_at: "2026-01-01T00:00:01Z".into(),
        },
    ];
    let text = translation_source_text(&segments);
    assert_eq!(
        text,
        "First clean fictional sentence.\nSecond clean fictional sentence."
    );
    assert!(!text.contains("[57-20067 ms]"));
    assert!(!text.contains("LocalMicrophone"));
    assert!(!text.contains("RemoteSystemAudio"));
}

#[test]
fn human_readable_export_omits_internal_identifiers_and_synthetic_prefixes() {
    let detail = export_detail();
    let markdown = export::render_user_export(&detail, ExportFormat::Markdown, true);
    assert!(markdown.contains("Fictional transcript sentence."));
    assert!(!markdown.contains("segment-1"));
    assert!(!markdown.contains("source-1"));
    assert!(!markdown.contains("[57-20067 ms]"));
}

#[test]
fn bundled_json_schemas_are_valid_and_use_stable_urn_identifiers() {
    let schemas = [
        include_str!("../../../../packages/schemas/analysis-artifacts.schema.json"),
        include_str!("../../../../packages/schemas/provider-capabilities.schema.json"),
        include_str!("../../../../packages/schemas/literal-translation.schema.json"),
        include_str!("../../../../packages/schemas/segment-understanding.schema.json"),
        include_str!("../../../../packages/schemas/post-meeting-analysis.schema.json"),
        include_str!("../../../../packages/schemas/communication-review.schema.json"),
        include_str!("../../../../packages/schemas/intelligent-comparison.schema.json"),
        include_str!("../../../../packages/schemas/meeting-minutes.schema.json"),
    ];
    for schema in schemas {
        let parsed: Value = serde_json::from_str(schema).expect("schema is valid JSON");
        if let Some(id) = parsed.get("$id").and_then(Value::as_str) {
            assert!(id.starts_with("urn:accordmesh:schema:"));
        }
    }
}

#[test]
fn project_title_contract_counts_unicode_without_silent_truncation() {
    let exact = "界".repeat(PROJECT_TITLE_MAX_CHARS);
    assert_eq!(
        normalized_project_title(Some(&exact)).expect("exact limit is accepted"),
        exact
    );

    let too_long = "界".repeat(PROJECT_TITLE_MAX_CHARS + 1);
    assert!(matches!(
        normalized_project_title(Some(&too_long)),
        Err(AppError::Stable("ERR_TITLE_TOO_LONG"))
    ));
    assert_eq!(
        normalized_project_title(Some("  Clear title  ")).expect("title is normalized"),
        "Clear title"
    );
}

#[tokio::test]
async fn failed_upload_media_reaches_a_terminal_visible_status() {
    let dir = ScopedDir::new("media-terminal-state");
    let repository = Repository::new(dir.0.join("app-data")).expect("create repository");
    repository.initialize().expect("initialize repository");
    let master = crypto::random_key();
    let project = repository
        .create_project(
            "Attachment terminal state",
            ProjectOrigin::RealtimeInPerson,
            ProjectStatus::Completed,
            &master,
        )
        .expect("create project");
    let project_key = repository
        .project_key(&project.id, &master)
        .expect("unwrap project key");
    let job_id = repository
        .queue_job(&project.id, None, "upload", 0, &json!({}))
        .expect("queue upload job");
    let queued_file_id = "queued-file-1";
    let source = dir.0.join("fictional-audio.wav");
    fs::write(&source, b"fictional encrypted media payload").expect("write media source");

    let imported = repository
        .import_media_asset(
            &project.id,
            &job_id,
            queued_file_id,
            "fictional-audio.wav",
            MediaKind::Audio,
            Some("audio/wav".into()),
            &source,
            &project_key,
        )
        .await
        .expect("import managed media");
    assert_eq!(imported.processing_status, "processing");

    assert_eq!(
        repository
            .finalize_incomplete_media_for_job(&job_id, "attached")
            .expect("finalize incomplete media"),
        1
    );
    let visible = repository
        .media_for_job(&job_id, queued_file_id)
        .expect("read media state")
        .expect("media exists");
    assert_eq!(visible.processing_status, "attached");
    assert_eq!(
        repository
            .finalize_incomplete_media_for_job(&job_id, "failed")
            .expect("terminal status remains stable"),
        0
    );
}

#[test]
fn export_safe_name_preserves_unicode_meeting_titles() {
    let title = "会议Alpha日文テストEnglish记录議事録Review確認同步Export文件名保存验证";
    let safe = export::safe_name(title);
    assert!(safe.contains("会议Alpha日文テストEnglish记录議事録Review確認"));
    assert!(!safe.starts_with('_'));
    assert!(!safe.chars().all(|character| character == '_'));
}

#[test]
fn export_safe_name_replaces_only_unsafe_filename_characters() {
    let safe = export::safe_name("客户会议/Legal:Review?確認|Minutes");
    assert_eq!(safe, "客户会议_Legal_Review_確認_Minutes");
}
