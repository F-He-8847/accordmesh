use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{multipart, Client, Response, Url};
use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::projects::types::{TimelineSegment, TrackRole};
use crate::providers::registry::full_capabilities;
use crate::providers::*;

pub struct OpenAiProvider {
    api_key: Zeroizing<String>,
    base_url: String,
    transcription_model: String,
    analysis_model: String,
    client: Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranscriptionContract {
    JsonText,
    WhisperVerboseSegments,
}

pub fn definition() -> ProviderDefinition {
    let mut capabilities = full_capabilities(&[
        "mp3", "mp4", "mpeg", "mpga", "m4a", "wav", "webm", "txt", "srt", "vtt",
    ]);
    capabilities.supports_streaming = false;
    ProviderDefinition {
        id: "openai".into(),
        display_name_key: "providers.openai.displayName".into(),
        credential_schema: vec![ProviderField {
            id: "apiKey".into(),
            label_key: "providers.fields.apiKey".into(),
            field_type: "password".into(),
            required: true,
            secret: true,
            default_value: None,
        }],
        configuration_schema: vec![
            ProviderField {
                id: "baseUrl".into(),
                label_key: "providers.fields.baseUrl".into(),
                field_type: "text".into(),
                required: true,
                secret: false,
                default_value: Some("https://api.openai.com/v1".into()),
            },
            ProviderField {
                id: "transcriptionModel".into(),
                label_key: "providers.fields.transcriptionModel".into(),
                field_type: "text".into(),
                required: true,
                secret: false,
                default_value: Some("gpt-4o-mini-transcribe".into()),
            },
            ProviderField {
                id: "analysisModel".into(),
                label_key: "providers.fields.analysisModel".into(),
                field_type: "text".into(),
                required: true,
                secret: false,
                default_value: Some("gpt-5-mini".into()),
            },
        ],
        model_assignments: vec![
            ProviderModelAssignment {
                capability: "fileTranscription".into(),
                configuration_field_id: "transcriptionModel".into(),
            },
            ProviderModelAssignment {
                capability: "realtimeTranscription".into(),
                configuration_field_id: "transcriptionModel".into(),
            },
            ProviderModelAssignment {
                capability: "textTranslation".into(),
                configuration_field_id: "analysisModel".into(),
            },
            ProviderModelAssignment {
                capability: "segmentUnderstanding".into(),
                configuration_field_id: "analysisModel".into(),
            },
            ProviderModelAssignment {
                capability: "meetingSynthesis".into(),
                configuration_field_id: "analysisModel".into(),
            },
            ProviderModelAssignment {
                capability: "communicationReview".into(),
                configuration_field_id: "analysisModel".into(),
            },
            ProviderModelAssignment {
                capability: "comparisonReport".into(),
                configuration_field_id: "analysisModel".into(),
            },
            ProviderModelAssignment {
                capability: "meetingMinutes".into(),
                configuration_field_id: "analysisModel".into(),
            },
        ],
        capabilities,
    }
}

pub fn validate_configuration(value: &Value) -> ProviderResult<()> {
    configuration_parts(value).map(|_| ())
}

fn configuration_parts(value: &Value) -> ProviderResult<(String, String, String, String)> {
    let api_key = value
        .get("apiKey")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("ERR_PROVIDER_NOT_CONFIGURED")?
        .to_owned();
    let base_url = value
        .get("baseUrl")
        .and_then(Value::as_str)
        .unwrap_or("https://api.openai.com/v1")
        .trim()
        .trim_end_matches('/')
        .to_owned();
    let transcription_model = value
        .get("transcriptionModel")
        .and_then(Value::as_str)
        .unwrap_or("gpt-4o-mini-transcribe")
        .trim()
        .to_owned();
    let analysis_model = value
        .get("analysisModel")
        .and_then(Value::as_str)
        .unwrap_or("gpt-5-mini")
        .trim()
        .to_owned();
    if base_url.is_empty() || transcription_model.is_empty() || analysis_model.is_empty() {
        return Err("ERR_PROVIDER_CONFIG");
    }
    transcription_contract(&transcription_model)?;
    let parsed = Url::parse(&base_url).map_err(|_| "ERR_PROVIDER_CONFIG")?;
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("ERR_PROVIDER_CONFIG");
    }
    let local_http = parsed.scheme() == "http"
        && matches!(
            parsed.host_str(),
            Some("127.0.0.1") | Some("localhost") | Some("::1")
        );
    if parsed.host_str().is_none() || !(parsed.scheme() == "https" || local_http) {
        return Err("ERR_PROVIDER_CONFIG");
    }
    Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|_| "ERR_PROVIDER_CONFIG")?;
    Ok((api_key, base_url, transcription_model, analysis_model))
}

fn transcription_contract(model: &str) -> ProviderResult<TranscriptionContract> {
    let model = model.trim().to_ascii_lowercase();
    if model.contains("transcribe-diarize") {
        return Err("ERR_PROVIDER_UNSUPPORTED_CAPABILITY");
    }
    if model == "whisper-1" {
        Ok(TranscriptionContract::WhisperVerboseSegments)
    } else {
        Ok(TranscriptionContract::JsonText)
    }
}

impl OpenAiProvider {
    pub fn from_configuration(value: Value) -> ProviderResult<Self> {
        let (api_key, base_url, transcription_model, analysis_model) = configuration_parts(&value)?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|_| "ERR_PROVIDER_CONFIG")?;
        Ok(Self {
            api_key: Zeroizing::new(api_key),
            base_url,
            transcription_model,
            analysis_model,
            client,
        })
    }

    async fn send(
        &self,
        builder: reqwest::RequestBuilder,
        context: &ProviderContext,
    ) -> ProviderResult<Response> {
        if context.cancelled.load(Ordering::Relaxed) {
            return Err("ERR_JOB_CANCELLED");
        }
        let future = builder.bearer_auth(self.api_key.as_str()).send();
        tokio::pin!(future);
        loop {
            tokio::select! {
                result = &mut future => return result.map_err(map_transport).and_then(check_status),
                _ = tokio::time::sleep(Duration::from_millis(150)) => {
                    if context.cancelled.load(Ordering::Relaxed) {
                        return Err("ERR_JOB_CANCELLED");
                    }
                }
            }
        }
    }

    async fn response_json(
        &self,
        response: Response,
        context: &ProviderContext,
    ) -> ProviderResult<Value> {
        if context.cancelled.load(Ordering::Relaxed) {
            return Err("ERR_JOB_CANCELLED");
        }
        let future = response.bytes();
        tokio::pin!(future);
        let bytes = loop {
            tokio::select! {
                result=&mut future=>break result.map_err(map_transport)?,
                _=tokio::time::sleep(Duration::from_millis(150))=>{
                    if context.cancelled.load(Ordering::Relaxed){return Err("ERR_JOB_CANCELLED");}
                }
            }
        };
        serde_json::from_slice(&bytes).map_err(|_| "ERR_PROVIDER_RESPONSE")
    }

    async fn structured(
        &self,
        kind: &'static str,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        let schema = schema_for(kind)?;
        let prompt = prompt_for(kind)?;
        let selected_model = context
            .model_override
            .as_deref()
            .unwrap_or(&self.analysis_model);
        let body = json!({
            "model": selected_model,
            "store": false,
            "input": [
                {"role":"system","content":[{"type":"input_text","text":prompt}]},
                {"role":"user","content":[{"type":"input_text","text":format!(
                    "Output language: {}\n\nSource:\n{}\n\nContext:\n{}",
                    input.output_language, input.source_text, input.context_json
                )}]}
            ],
            "text":{"format":{"type":"json_schema","name":kind,"strict":true,"schema":schema}}
        });
        let response = self
            .send(
                self.client
                    .post(format!("{}/responses", self.base_url))
                    .json(&body),
                context,
            )
            .await?;
        let value = self.response_json(response, context).await?;
        let text = extract_output_text(&value).ok_or("ERR_PROVIDER_RESPONSE")?;
        let payload: Value = serde_json::from_str(text).map_err(|_| "ERR_PROVIDER_SCHEMA")?;
        crate::analysis::validate_artifact(kind, &payload)?;
        Ok(GeneratedDraft {
            artifact_type: kind,
            model_id: selected_model.to_owned(),
            prompt_version: prompt_version(kind),
            schema_version: schema_version(kind),
            payload,
        })
    }
}

impl Provider for OpenAiProvider {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        let mut capabilities = full_capabilities(&[
            "mp3", "mp4", "mpeg", "mpga", "m4a", "wav", "webm", "txt", "srt", "vtt",
        ]);
        capabilities.supports_streaming = false;
        capabilities
    }

    fn model_id_for(&self, capability: &str) -> String {
        if capability == "file_transcription" || capability == "realtime_transcription" {
            self.transcription_model.clone()
        } else {
            self.analysis_model.clone()
        }
    }
}

#[async_trait]
impl FileTranscriptionProvider for OpenAiProvider {
    async fn transcribe_file(
        &self,
        input: &TranscriptionInput<'_>,
        context: &ProviderContext,
    ) -> ProviderResult<Vec<TranscriptDraft>> {
        let metadata = tokio::fs::metadata(input.path)
            .await
            .map_err(|_| "ERR_MEDIA_READ")?;
        if !metadata.is_file()
            || metadata.len() == 0
            || metadata.len() >= crate::media::TRANSCRIPTION_UPLOAD_LIMIT_BYTES
        {
            return Err("ERR_MEDIA_SIZE");
        }

        let contract = transcription_contract(&self.transcription_model)?;
        let mut part = multipart::Part::file(input.path)
            .await
            .map_err(|_| "ERR_MEDIA_READ")?
            .file_name(input.original_file_name.to_string());
        if let Some(mime_type) = input.mime_type {
            part = part.mime_str(mime_type).map_err(|_| "ERR_MEDIA_INVALID")?;
        }
        let mut form = multipart::Form::new()
            .text("model", self.transcription_model.clone())
            .text(
                "response_format",
                match contract {
                    TranscriptionContract::JsonText => "json",
                    TranscriptionContract::WhisperVerboseSegments => "verbose_json",
                },
            )
            .part("file", part);
        if contract == TranscriptionContract::WhisperVerboseSegments {
            form = form.text("timestamp_granularities[]", "segment");
        }
        if let Some(language) = context
            .source_language
            .as_deref()
            .and_then(normalize_language_hint)
        {
            form = form.text("language", language);
        }

        let response = self
            .send(
                self.client
                    .post(format!("{}/audio/transcriptions", self.base_url))
                    .multipart(form),
                context,
            )
            .await?;
        let value = self.response_json(response, context).await?;
        parse_transcription_response(&value, contract, input)
    }
}

#[async_trait]
impl RealtimeTranscriptionProvider for OpenAiProvider {
    async fn transcribe_realtime_chunk(
        &self,
        path: &Path,
        offset: i64,
        track: TrackRole,
        context: &ProviderContext,
    ) -> ProviderResult<Vec<TranscriptDraft>> {
        let end_ms = pcm_wav_duration_ms(path).map(|duration| offset.saturating_add(duration));
        self.transcribe_file(
            &TranscriptionInput {
                path,
                original_file_name: "realtime.wav",
                mime_type: Some("audio/wav"),
                offset_ms: offset,
                end_ms,
                track_role: track,
                chunk_index: 0,
            },
            context,
        )
        .await
    }
}

#[async_trait]
impl TranslationProvider for OpenAiProvider {
    async fn translate(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.structured("literal_translation", input, context).await
    }
}

#[async_trait]
impl MeetingSynthesisProvider for OpenAiProvider {
    async fn synthesize_meeting(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.structured("post_meeting_analysis", input, context)
            .await
    }
}

#[async_trait]
impl CommunicationReviewProvider for OpenAiProvider {
    async fn review_communication(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.structured("communication_review", input, context)
            .await
    }
}

#[async_trait]
impl ComparisonReportProvider for OpenAiProvider {
    async fn compare(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.structured("intelligent_comparison_report", input, context)
            .await
    }
}

#[async_trait]
impl MeetingMinutesProvider for OpenAiProvider {
    async fn minutes(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.structured("meeting_minutes", input, context).await
    }
}

#[async_trait]
impl SegmentUnderstandingProvider for OpenAiProvider {
    async fn understand_segment(
        &self,
        segment: &TimelineSegment,
        language: &str,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.structured(
            "segment_understanding",
            &GenerationInput {
                project_id: segment.project_id.clone(),
                source_ids: vec![segment.id.clone()],
                source_text: segment.source_transcript.clone(),
                output_language: language.into(),
                context_json: json!({
                    "trackRole": segment.track_role,
                    "startMs": segment.start_ms,
                    "endMs": segment.end_ms
                }),
            },
            context,
        )
        .await
    }
}

fn parse_transcription_response(
    value: &Value,
    contract: TranscriptionContract,
    input: &TranscriptionInput<'_>,
) -> ProviderResult<Vec<TranscriptDraft>> {
    let detected_language = value
        .get("language")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    if contract == TranscriptionContract::WhisperVerboseSegments {
        let segments = value
            .get("segments")
            .and_then(Value::as_array)
            .filter(|segments| !segments.is_empty())
            .ok_or("ERR_PROVIDER_RESPONSE")?;
        let mut drafts = Vec::with_capacity(segments.len());
        for segment in segments {
            let start = finite_nonnegative_number(segment.get("start"))?;
            let end = finite_nonnegative_number(segment.get("end"))?;
            if end <= start {
                return Err("ERR_PROVIDER_RESPONSE");
            }
            let text = segment
                .get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .ok_or("ERR_PROVIDER_RESPONSE")?;
            let start_ms = input
                .offset_ms
                .saturating_add((start * 1000.0).round() as i64);
            let mut end_ms = input
                .offset_ms
                .saturating_add((end * 1000.0).round() as i64);
            if let Some(limit) = input.end_ms {
                if start_ms >= limit {
                    return Err("ERR_PROVIDER_RESPONSE");
                }
                end_ms = end_ms.min(limit);
            }
            if end_ms <= start_ms {
                return Err("ERR_PROVIDER_RESPONSE");
            }
            drafts.push(TranscriptDraft {
                start_ms,
                end_ms,
                text: text.to_owned(),
                detected_language: detected_language.clone(),
                confidence: None,
            });
        }
        return Ok(drafts);
    }

    let text = value
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or("ERR_PROVIDER_RESPONSE")?;
    let end_ms = input
        .end_ms
        .filter(|end| *end > input.offset_ms)
        .unwrap_or_else(|| input.offset_ms.saturating_add(1));
    Ok(vec![TranscriptDraft {
        start_ms: input.offset_ms,
        end_ms,
        text: text.to_owned(),
        detected_language,
        confidence: None,
    }])
}

fn finite_nonnegative_number(value: Option<&Value>) -> ProviderResult<f64> {
    let value = value
        .and_then(Value::as_f64)
        .ok_or("ERR_PROVIDER_RESPONSE")?;
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err("ERR_PROVIDER_RESPONSE")
    }
}

fn normalize_language_hint(value: &str) -> Option<String> {
    let primary = value
        .trim()
        .split(|character| character == '-' || character == '_')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    (matches!(primary.len(), 2 | 3)
        && primary
            .chars()
            .all(|character| character.is_ascii_alphabetic()))
    .then_some(primary)
}

fn pcm_wav_duration_ms(path: &Path) -> Option<i64> {
    let mut header = [0u8; 44];
    let mut file = File::open(path).ok()?;
    file.read_exact(&mut header).ok()?;
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" || &header[12..16] != b"fmt " {
        return None;
    }
    let channels = u16::from_le_bytes([header[22], header[23]]) as u64;
    let sample_rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]) as u64;
    let bits_per_sample = u16::from_le_bytes([header[34], header[35]]) as u64;
    if &header[36..40] != b"data" || channels == 0 || sample_rate == 0 || bits_per_sample == 0 {
        return None;
    }
    let data_bytes = u32::from_le_bytes([header[40], header[41], header[42], header[43]]) as u64;
    let bytes_per_second = sample_rate
        .checked_mul(channels)?
        .checked_mul(bits_per_sample)?
        .checked_div(8)?;
    if bytes_per_second == 0 {
        return None;
    }
    Some(
        data_bytes
            .saturating_mul(1000)
            .checked_div(bytes_per_second)? as i64,
    )
}

fn check_status(response: Response) -> ProviderResult<Response> {
    match response.status().as_u16() {
        200..=299 => Ok(response),
        401 | 403 => Err("ERR_PROVIDER_AUTH"),
        408 => Err("ERR_PROVIDER_TIMEOUT"),
        429 => Err("ERR_PROVIDER_QUOTA"),
        500..=599 => Err("ERR_PROVIDER_UNAVAILABLE"),
        _ => Err("ERR_PROVIDER_REQUEST"),
    }
}

fn map_transport(error: reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "ERR_PROVIDER_TIMEOUT"
    } else if error.is_connect() {
        "ERR_PROVIDER_UNAVAILABLE"
    } else {
        "ERR_PROVIDER_REQUEST"
    }
}

fn extract_output_text(value: &Value) -> Option<&str> {
    value
        .get("output")?
        .as_array()?
        .iter()
        .flat_map(|value| {
            value
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find_map(|item| {
            if item.get("type").and_then(Value::as_str) == Some("output_text") {
                item.get("text").and_then(Value::as_str)
            } else {
                None
            }
        })
}

fn prompt_for(kind: &str) -> ProviderResult<&'static str> {
    match kind {
        "literal_translation" => Ok(include_str!(
            "../../../../../../packages/prompts/literal-translation-v1.md"
        )),
        "segment_understanding" => Ok(include_str!(
            "../../../../../../packages/prompts/segment-understanding-v1.md"
        )),
        "post_meeting_analysis" => Ok(include_str!(
            "../../../../../../packages/prompts/post-meeting-analysis-v1.md"
        )),
        "communication_review" => Ok(include_str!(
            "../../../../../../packages/prompts/communication-review-v1.md"
        )),
        "intelligent_comparison_report" => Ok(include_str!(
            "../../../../../../packages/prompts/intelligent-comparison-v1.md"
        )),
        "meeting_minutes" => Ok(include_str!(
            "../../../../../../packages/prompts/meeting-minutes-v1.md"
        )),
        _ => Err("ERR_PROVIDER_UNSUPPORTED_CAPABILITY"),
    }
}

fn schema_for(kind: &str) -> ProviderResult<Value> {
    let raw = match kind {
        "literal_translation" => {
            include_str!("../../../../../../packages/schemas/literal-translation.schema.json")
        }
        "segment_understanding" => {
            include_str!("../../../../../../packages/schemas/segment-understanding.schema.json")
        }
        "post_meeting_analysis" => {
            include_str!("../../../../../../packages/schemas/post-meeting-analysis.schema.json")
        }
        "communication_review" => {
            include_str!("../../../../../../packages/schemas/communication-review.schema.json")
        }
        "intelligent_comparison_report" => {
            include_str!("../../../../../../packages/schemas/intelligent-comparison.schema.json")
        }
        "meeting_minutes" => {
            include_str!("../../../../../../packages/schemas/meeting-minutes.schema.json")
        }
        _ => return Err("ERR_PROVIDER_UNSUPPORTED_CAPABILITY"),
    };
    serde_json::from_str(raw).map_err(|_| "ERR_PROVIDER_SCHEMA")
}

fn prompt_version(kind: &str) -> &'static str {
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

fn schema_version(kind: &str) -> &'static str {
    prompt_version(kind)
}
