use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectOrigin {
    RealtimeOnline,
    RealtimeInPerson,
    UploadOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeMode {
    Online,
    InPerson,
}

impl From<RealtimeMode> for ProjectOrigin {
    fn from(value: RealtimeMode) -> Self {
        match value {
            RealtimeMode::Online => ProjectOrigin::RealtimeOnline,
            RealtimeMode::InPerson => ProjectOrigin::RealtimeInPerson,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Completed,
    Processing,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeSessionStatus {
    Starting,
    Running,
    Paused,
    Completed,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackRole {
    RemoteSystemAudio,
    LocalMicrophone,
    RoomMicrophone,
    UploadedMedia,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Audio,
    Video,
    Transcript,
    Subtitle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Markdown,
    Txt,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingProject {
    pub id: String,
    pub title: String,
    pub origin: ProjectOrigin,
    pub status: ProjectStatus,
    pub created_at: String,
    pub updated_at: String,
    pub realtime_session_id: Option<String>,
    pub media_asset_ids: Vec<String>,
    pub timeline_segment_ids: Vec<String>,
    pub artifact_ids: Vec<String>,
    pub generation_run_ids: Vec<String>,
    pub has_comparison: bool,
    pub has_minutes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineSegment {
    pub id: String,
    pub project_id: String,
    pub source_id: String,
    pub track_role: TrackRole,
    pub start_ms: i64,
    pub end_ms: i64,
    pub source_transcript: String,
    pub detected_language: Option<String>,
    pub transcript_status: String,
    pub confidence: Option<f32>,
    pub warnings: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAsset {
    pub id: String,
    pub project_id: String,
    pub kind: MediaKind,
    pub original_file_name: String,
    pub imported_at: String,
    pub duration_ms: Option<i64>,
    pub sha256: String,
    pub processing_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisArtifact {
    pub id: String,
    pub project_id: String,
    pub artifact_type: String,
    pub source_ids: Vec<String>,
    pub schema_version: String,
    pub prompt_version: String,
    pub provider_id: String,
    pub model_id: String,
    pub app_version: String,
    pub created_at: String,
    pub status: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationRun {
    pub id: String,
    pub project_id: String,
    pub artifact_id: Option<String>,
    pub provider_id: String,
    pub model_id: String,
    pub prompt_version: String,
    pub schema_version: String,
    pub source_ids: Vec<String>,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeSession {
    pub id: String,
    pub project_id: String,
    pub mode: RealtimeMode,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: RealtimeSessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingJob {
    pub id: String,
    pub project_id: Option<String>,
    pub asset_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub stage: String,
    pub progress: f64,
    pub priority: i64,
    pub retry_count: i64,
    pub error_code: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    pub project: MeetingProject,
    pub timeline: Vec<TimelineSegment>,
    pub media_assets: Vec<MediaAsset>,
    pub artifacts: Vec<AnalysisArtifact>,
    pub generation_runs: Vec<GenerationRun>,
    pub realtime_session: Option<RealtimeSession>,
    pub jobs: Vec<ProcessingJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedFile {
    pub selection_token: String,
    pub original_file_name: String,
    pub kind: MediaKind,
    pub size: u64,
    pub mime_type: Option<String>,
}
