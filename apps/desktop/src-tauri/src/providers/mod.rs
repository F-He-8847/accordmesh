pub mod mock;
pub mod openai;
pub mod registry;
pub mod test_adapter;

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::projects::types::{TimelineSegment, TrackRole};

pub type ProviderResult<T> = Result<T, &'static str>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub file_transcription: bool,
    pub realtime_transcription: bool,
    pub text_translation: bool,
    pub segment_understanding: bool,
    pub meeting_synthesis: bool,
    pub communication_review: bool,
    pub comparison_report: bool,
    pub meeting_minutes: bool,
    pub supports_streaming: bool,
    pub supports_structured_output: bool,
    pub supports_language_auto_detection: bool,
    pub supports_code_switching: bool,
    pub supported_input_formats: Vec<String>,
    pub supported_source_languages: Vec<String>,
    pub supported_target_languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderField {
    pub id: String,
    pub label_key: String,
    pub field_type: String,
    pub required: bool,
    pub secret: bool,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelAssignment {
    pub capability: String,
    pub configuration_field_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDefinition {
    pub id: String,
    pub display_name_key: String,
    pub credential_schema: Vec<ProviderField>,
    pub configuration_schema: Vec<ProviderField>,
    pub model_assignments: Vec<ProviderModelAssignment>,
    pub capabilities: ProviderCapabilities,
}

#[derive(Debug, Clone)]
pub struct ProviderContext {
    pub cancelled: Arc<AtomicBool>,
    pub source_language: Option<String>,
    pub model_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptDraft {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub detected_language: Option<String>,
    pub confidence: Option<f32>,
}

pub struct TranscriptionInput<'a> {
    pub path: &'a Path,
    pub original_file_name: &'a str,
    pub mime_type: Option<&'a str>,
    pub offset_ms: i64,
    pub end_ms: Option<i64>,
    pub track_role: TrackRole,
    pub chunk_index: i64,
}

#[derive(Debug, Clone)]
pub struct GenerationInput {
    pub project_id: String,
    pub source_ids: Vec<String>,
    pub source_text: String,
    pub output_language: String,
    pub context_json: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct GeneratedDraft {
    pub artifact_type: &'static str,
    pub model_id: String,
    pub prompt_version: &'static str,
    pub schema_version: &'static str,
    pub payload: serde_json::Value,
}

#[async_trait]
pub trait FileTranscriptionProvider: Send + Sync {
    async fn transcribe_file(
        &self,
        input: &TranscriptionInput<'_>,
        context: &ProviderContext,
    ) -> ProviderResult<Vec<TranscriptDraft>>;
}

#[async_trait]
pub trait RealtimeTranscriptionProvider: Send + Sync {
    async fn transcribe_realtime_chunk(
        &self,
        pcm_wav: &Path,
        offset_ms: i64,
        track: TrackRole,
        context: &ProviderContext,
    ) -> ProviderResult<Vec<TranscriptDraft>>;
}

#[async_trait]
pub trait TranslationProvider: Send + Sync {
    async fn translate(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft>;
}

#[async_trait]
pub trait SegmentUnderstandingProvider: Send + Sync {
    async fn understand_segment(
        &self,
        segment: &TimelineSegment,
        language: &str,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft>;
}

#[async_trait]
pub trait MeetingSynthesisProvider: Send + Sync {
    async fn synthesize_meeting(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft>;
}

#[async_trait]
pub trait CommunicationReviewProvider: Send + Sync {
    async fn review_communication(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft>;
}

#[async_trait]
pub trait ComparisonReportProvider: Send + Sync {
    async fn compare(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft>;
}

#[async_trait]
pub trait MeetingMinutesProvider: Send + Sync {
    async fn minutes(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft>;
}

pub trait Provider:
    FileTranscriptionProvider
    + RealtimeTranscriptionProvider
    + TranslationProvider
    + SegmentUnderstandingProvider
    + MeetingSynthesisProvider
    + CommunicationReviewProvider
    + ComparisonReportProvider
    + MeetingMinutesProvider
{
    fn id(&self) -> &'static str;
    fn capabilities(&self) -> ProviderCapabilities;
    fn model_id_for(&self, capability: &str) -> String;
}
