use std::path::Path;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use serde_json::json;

use crate::projects::types::{TimelineSegment, TrackRole};
use crate::providers::registry::full_capabilities;
use crate::providers::*;

#[derive(Debug, Clone, Copy)]
enum Scenario {
    Normal,
    Timeout,
    Quota,
    Authentication,
    Unavailable,
    Unsupported,
    #[cfg(test)]
    CancelAfterTranslation,
    #[cfg(test)]
    FailMeetingSynthesis,
    #[cfg(test)]
    RequireSourceArtifactPayloads,
}

#[derive(Clone)]
pub struct MockProvider {
    scenario: Scenario,
}

impl MockProvider {
    pub fn from_configuration(value: &serde_json::Value) -> Self {
        let scenario = match value
            .get("scenario")
            .and_then(|v| v.as_str())
            .unwrap_or("normal")
        {
            "timeout" => Scenario::Timeout,
            "quota" => Scenario::Quota,
            "authentication" => Scenario::Authentication,
            "unavailable" => Scenario::Unavailable,
            "unsupported" => Scenario::Unsupported,
            #[cfg(test)]
            "cancel_after_translation" => Scenario::CancelAfterTranslation,
            #[cfg(test)]
            "fail_meeting_synthesis" => Scenario::FailMeetingSynthesis,
            #[cfg(test)]
            "require_source_artifact_payloads" => Scenario::RequireSourceArtifactPayloads,
            _ => Scenario::Normal,
        };
        Self { scenario }
    }

    pub fn definition() -> ProviderDefinition {
        ProviderDefinition {
            id: "mock".into(),
            display_name_key: "providers.mock.displayName".into(),
            credential_schema: vec![],
            configuration_schema: vec![ProviderField {
                id: "scenario".into(),
                label_key: "providers.fields.mockScenario".into(),
                field_type: "select".into(),
                required: false,
                secret: false,
                default_value: Some("normal".into()),
            }],
            model_assignments: vec![],
            capabilities: full_capabilities(&["audio", "video", "transcript", "subtitle", "text"]),
        }
    }

    fn check(&self, context: &ProviderContext) -> ProviderResult<()> {
        if context.cancelled.load(Ordering::Relaxed) {
            return Err("ERR_JOB_CANCELLED");
        }
        match self.scenario {
            Scenario::Normal => Ok(()),
            Scenario::Timeout => Err("ERR_PROVIDER_TIMEOUT"),
            Scenario::Quota => Err("ERR_PROVIDER_QUOTA"),
            Scenario::Authentication => Err("ERR_PROVIDER_AUTH"),
            Scenario::Unavailable => Err("ERR_PROVIDER_UNAVAILABLE"),
            Scenario::Unsupported => Err("ERR_PROVIDER_UNSUPPORTED_CAPABILITY"),
            #[cfg(test)]
            Scenario::CancelAfterTranslation => Ok(()),
            #[cfg(test)]
            Scenario::FailMeetingSynthesis => Ok(()),
            #[cfg(test)]
            Scenario::RequireSourceArtifactPayloads => Ok(()),
        }
    }

    fn draft(
        &self,
        artifact_type: &'static str,
        model: &str,
        payload: serde_json::Value,
    ) -> GeneratedDraft {
        GeneratedDraft {
            artifact_type,
            model_id: model.into(),
            prompt_version: prompt_version(artifact_type),
            schema_version: schema_version(artifact_type),
            payload,
        }
    }
}

impl Provider for MockProvider {
    fn id(&self) -> &'static str {
        "mock"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        let mut c = full_capabilities(&["audio", "video", "transcript", "subtitle", "text"]);
        if matches!(self.scenario, Scenario::Unsupported) {
            c.comparison_report = false;
        }
        c
    }
    fn model_id_for(&self, capability: &str) -> String {
        match capability {
            "text_translation" => "mock-translation-v1",
            "segment_understanding" => "mock-understanding-v1",
            "meeting_synthesis" => "mock-analysis-v1",
            "communication_review" => "mock-review-v1",
            "comparison_report" => "mock-comparison-v1",
            "meeting_minutes" => "mock-minutes-v1",
            _ => return format!("mock-{capability}-v1"),
        }
        .into()
    }
}

#[async_trait]
impl FileTranscriptionProvider for MockProvider {
    async fn transcribe_file(
        &self,
        input: &TranscriptionInput<'_>,
        context: &ProviderContext,
    ) -> ProviderResult<Vec<TranscriptDraft>> {
        self.check(context)?;
        Ok(mock_transcript(input.offset_ms))
    }
}

#[async_trait]
impl RealtimeTranscriptionProvider for MockProvider {
    async fn transcribe_realtime_chunk(
        &self,
        _: &Path,
        offset: i64,
        _: TrackRole,
        context: &ProviderContext,
    ) -> ProviderResult<Vec<TranscriptDraft>> {
        self.check(context)?;
        Ok(mock_transcript(offset))
    }
}

#[async_trait]
impl TranslationProvider for MockProvider {
    async fn translate(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.check(context)?;
        #[cfg(test)]
        if matches!(self.scenario, Scenario::CancelAfterTranslation) {
            context.cancelled.store(true, Ordering::Relaxed);
        }
        Ok(self.draft("literal_translation","mock-translation-v1",json!({"targetLanguage":input.output_language,"translatedText":input.source_text,"preservesSource":true})))
    }
}

#[async_trait]
impl SegmentUnderstandingProvider for MockProvider {
    async fn understand_segment(
        &self,
        segment: &TimelineSegment,
        language: &str,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.check(context)?;
        Ok(self.draft("segment_understanding","mock-understanding-v1",json!({"segmentId":segment.id,"language":language,"coreMeaning":"A condition must be clarified before it is treated as a commitment.","explicitIntents":["clarify the stated condition"],"inferredIntents":[{"text":"The wording may be intended to reduce delivery risk.","confidence":"medium"}],"keyFacts":[{"text":segment.source_transcript,"kind":"explicit_fact"}],"ambiguities":["The approval order remains unresolved."],"guidance":{"answer":["Answer only what the evidence establishes."],"explain":["Separate pilot timing from public launch timing."],"ask":["Ask which review must happen first."],"confirm":["Confirm dates, conditions, and scope."]},"evidenceRefs":[evidence(segment)]})))
    }
}

#[async_trait]
impl MeetingSynthesisProvider for MockProvider {
    async fn synthesize_meeting(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.check(context)?;
        #[cfg(test)]
        if matches!(self.scenario, Scenario::FailMeetingSynthesis) {
            return Err("ERR_PROVIDER_TIMEOUT");
        }
        Ok(self.draft("post_meeting_analysis","mock-analysis-v1",json!({"language":input.output_language,"overview":"The meeting addressed pilot readiness, approval conditions, budget scope, and unresolved review timing.","majorTopics":["pilot readiness","security sign-off","translation review","legal review"],"keyFacts":["The candidate date is conditional.","No public launch commitment is established."],"confirmedDecisions":[],"conditions":["Security sign-off is required before relying on the pilot date."],"constraints":["Translation review must remain inside the agreed scope."],"unresolvedIssues":["Legal review timing remains unresolved."],"ambiguities":["Pilot and public launch timing may have been conflated."],"recommendedFollowUpActions":["Confirm security sign-off timing.","Confirm legal review order."],"uncertaintyNotes":["No owner or deadline is inferred."],"evidenceRefs":input.context_json.get("evidenceRefs").cloned().unwrap_or_else(||json!([]))})))
    }
}

#[async_trait]
impl CommunicationReviewProvider for MockProvider {
    async fn review_communication(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.check(context)?;
        Ok(self.draft("communication_review","mock-review-v1",json!({"scope":"meeting_level","observations":[{"type":"missing_confirmation","text":"The review order should be confirmed before the candidate date is treated as stable."},{"type":"unsupported_commitment","text":"Do not describe the public launch date as decided."}],"improvedWording":["To confirm: the pilot target remains conditional, and the public launch date is still open."],"evidenceRefs":input.context_json.get("evidenceRefs").cloned().unwrap_or_else(||json!([]))})))
    }
}

#[async_trait]
impl ComparisonReportProvider for MockProvider {
    async fn compare(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.check(context)?;
        let realtime = input
            .context_json
            .pointer("/realtimeEvidence/evidenceRefs/0")
            .cloned()
            .into_iter()
            .collect::<Vec<_>>();
        let uploaded = input
            .context_json
            .pointer("/uploadedEvidence/evidenceRefs/0")
            .cloned()
            .into_iter()
            .collect::<Vec<_>>();
        let both = realtime
            .iter()
            .chain(uploaded.iter())
            .cloned()
            .collect::<Vec<_>>();
        Ok(self.draft("intelligent_comparison_report","mock-comparison-v1",json!({"overallAssessment":"The recording confirms the caution in the real-time result and adds the exact approval condition.","correctlyCaptured":[{"type":"Condition","status":"confirmed","text":"Translation review scope affects budget.","evidenceRefs":both}],"missedOrIncomplete":[{"type":"Condition","status":"incomplete_in_realtime","text":"The candidate date depends on security sign-off.","evidenceRefs":uploaded}],"correctedInterpretations":[{"type":"Decision","status":"refined","text":"The date is a pilot target, not a public launch commitment.","evidenceRefs":both}],"newlyDiscovered":[{"type":"UnresolvedIssue","status":"new_detail","text":"Legal review timing remains unresolved.","evidenceRefs":uploaded}],"guidanceRevisions":[{"type":"Guidance","status":"guidance_changed","text":"Ask specifically about the security sign-off date.","evidenceRefs":both}],"conclusionChanges":[],"recommendedFollowUps":["Confirm pilot conditions.","Confirm legal review order."]})))
    }
}

#[async_trait]
impl MeetingMinutesProvider for MockProvider {
    async fn minutes(
        &self,
        input: &GenerationInput,
        context: &ProviderContext,
    ) -> ProviderResult<GeneratedDraft> {
        self.check(context)?;
        #[cfg(test)]
        if matches!(self.scenario, Scenario::RequireSourceArtifactPayloads) {
            let snapshots = input
                .context_json
                .get("sourceArtifacts")
                .and_then(|value| value.as_array())
                .ok_or("ERR_JOB_PAYLOAD")?;
            if snapshots.len() < 2
                || snapshots
                    .iter()
                    .any(|value| value.get("id").and_then(|item| item.as_str()).is_none())
                || snapshots.iter().any(|value| {
                    value
                        .get("artifactType")
                        .and_then(|item| item.as_str())
                        .is_none()
                })
                || snapshots.iter().any(|value| value.get("payload").is_none())
            {
                return Err("ERR_JOB_PAYLOAD");
            }
            let selected_markers = snapshots
                .iter()
                .filter_map(|value| {
                    value
                        .pointer("/payload/marker")
                        .and_then(|item| item.as_str())
                })
                .collect::<Vec<_>>();
            if selected_markers.len() != snapshots.len() {
                return Err("ERR_JOB_PAYLOAD");
            }
            return Ok(self.draft("meeting_minutes","mock-minutes-v1",json!({
                "projectId":input.project_id,
                "language":input.output_language,
                "sourceArtifactIds":input.context_json.get("sourceArtifactIds").cloned().unwrap_or_else(||json!([])),
                "sections":[{"title":"Overview","items":[format!("Selected upstream versions: {}",selected_markers.join(" | "))]}],
                "evidenceRefs":input.context_json.get("evidenceRefs").cloned().unwrap_or_else(||json!([])),
                "limitations":["Test-only provenance scenario."]
            })));
        }
        Ok(self.draft("meeting_minutes","mock-minutes-v1",json!({"projectId":input.project_id,"language":input.output_language,"sourceArtifactIds":input.context_json.get("sourceArtifactIds").cloned().unwrap_or_else(||json!([])),"sections":[{"title":"Overview","items":["Discussed pilot timing, review conditions, and translation scope."]},{"title":"Confirmed decisions","items":[]},{"title":"Open questions","items":["Security sign-off timing","Legal review order"]},{"title":"Follow-up","items":["Verify critical dates, conditions, owners, and commitments with participants."]}],"evidenceRefs":input.context_json.get("evidenceRefs").cloned().unwrap_or_else(||json!([])),"limitations":["No speaker identities, owners, or deadlines were inferred."]})))
    }
}

fn mock_transcript(offset: i64) -> Vec<TranscriptDraft> {
    [
        (
            0,
            8100,
            "The pilot can begin on May 12 if security signs off by the previous Friday.",
        ),
        (
            8300,
            16800,
            "No one committed to a final public launch date during this meeting.",
        ),
        (
            17100,
            24100,
            "The next step is to confirm legal review timing and translation review effort.",
        ),
    ]
    .into_iter()
    .map(|(s, e, text)| TranscriptDraft {
        start_ms: offset + s,
        end_ms: offset + e,
        text: text.into(),
        detected_language: Some("en".into()),
        confidence: Some(0.96),
    })
    .collect()
}
fn evidence(s: &TimelineSegment) -> serde_json::Value {
    json!({"sourceId":s.source_id,"segmentId":s.id,"startMs":s.start_ms,"endMs":s.end_ms,"evidenceType":"explicit_statement","confidence":"high"})
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
