use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::Value;

use crate::projects::types::{
    AnalysisArtifact, ExportFormat, MediaKind, ProjectDetail, ProjectOrigin, TimelineSegment,
    TrackRole,
};

const REPORT_ARTIFACT_ORDER: &[&str] = &[
    "meeting_minutes",
    "post_meeting_analysis",
    "communication_review",
    "intelligent_comparison_report",
];

pub async fn choose_and_write(
    detail: &ProjectDetail,
    format: ExportFormat,
    include_transcript: bool,
) -> Result<String, &'static str> {
    let (extension, label) = match format {
        ExportFormat::Markdown => ("md", "Markdown"),
        ExportFormat::Txt => ("txt", "Text"),
        ExportFormat::Json => ("json", "JSON"),
    };
    let content = render_user_export(detail, format, include_transcript);
    let file = rfd::AsyncFileDialog::new()
        .set_file_name(format!(
            "{}.{}",
            safe_name(&detail.project.title),
            extension
        ))
        .add_filter(label, &[extension])
        .save_file()
        .await
        .ok_or("ERR_EXPORT_CANCELLED")?;
    write(file.path(), content.as_bytes()).await?;
    Ok(file.path().to_string_lossy().to_string())
}

pub(crate) fn prepare_detail(
    mut detail: ProjectDetail,
    selected_artifact_ids: &[String],
) -> Result<ProjectDetail, &'static str> {
    if selected_artifact_ids.is_empty() {
        detail.artifacts.clear();
        detail.generation_runs.clear();
        detail.project.artifact_ids.clear();
        detail.project.generation_run_ids.clear();
        return Ok(detail);
    }

    let unique = selected_artifact_ids.iter().collect::<HashSet<_>>();
    if unique.len() != selected_artifact_ids.len() {
        return Err("ERR_JOB_PAYLOAD");
    }

    detail
        .artifacts
        .retain(|artifact| unique.contains(&artifact.id));
    if detail.artifacts.len() != selected_artifact_ids.len() {
        return Err("ERR_JOB_PAYLOAD");
    }

    detail.generation_runs.retain(|run| {
        run.artifact_id
            .as_ref()
            .is_some_and(|artifact_id| unique.contains(artifact_id))
    });
    detail.project.artifact_ids = selected_artifact_ids.to_vec();
    detail.project.generation_run_ids = detail
        .generation_runs
        .iter()
        .map(|run| run.id.clone())
        .collect();
    Ok(detail)
}

/// Compatibility renderer retained for selected-version and transcript-only exports.
/// Production exports use `render_user_export` below.
pub(crate) fn render(detail: &ProjectDetail, format: ExportFormat) -> String {
    match format {
        ExportFormat::Markdown => legacy_markdown(detail),
        ExportFormat::Txt => legacy_text(detail),
        ExportFormat::Json => json_export(detail),
    }
}

pub(crate) fn render_user_export(
    detail: &ProjectDetail,
    format: ExportFormat,
    include_transcript: bool,
) -> String {
    match format {
        ExportFormat::Markdown => report_markdown(detail, include_transcript),
        ExportFormat::Txt => report_text(detail, include_transcript),
        // JSON is always the complete structured audit export. The transcript toggle only
        // applies to the human-readable Markdown and TXT reports.
        ExportFormat::Json => json_export(detail),
    }
}

async fn write(path: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| "ERR_EXPORT_WRITE")?;
    }
    let temporary = path.with_extension("accordmesh.tmp");
    if tokio::fs::write(&temporary, bytes).await.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err("ERR_EXPORT_WRITE");
    }
    if tokio::fs::rename(&temporary, path).await.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err("ERR_EXPORT_WRITE");
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn write_for_test(path: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    write(path, bytes).await
}

pub(crate) fn safe_name(value: &str) -> String {
    let mut output = String::new();
    let mut previous_separator = false;
    for character in value.trim().chars() {
        let replacement = if character.is_control()
            || matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            ) {
            Some('_')
        } else {
            None
        };
        let next = replacement.unwrap_or(character);
        if next == '_' {
            if previous_separator {
                continue;
            }
            previous_separator = true;
        } else {
            previous_separator = false;
        }
        output.push(next);
        if output.chars().count() >= 96 {
            break;
        }
    }
    let name = output.trim().trim_matches('_').trim().to_owned();
    if name.is_empty() {
        "accordmesh-meeting".into()
    } else {
        name
    }
}

fn report_markdown(detail: &ProjectDetail, include_transcript: bool) -> String {
    let mut output = format!(
        "# {}\n\n- Meeting type: {}\n- Created: {}\n- Status: {}\n",
        detail.project.title,
        origin_label(detail.project.origin),
        detail.project.created_at,
        status_label(&detail.project.status)
    );

    append_report_artifacts_markdown(&mut output, detail);
    if include_transcript {
        append_transcript_markdown(&mut output, detail);
    }
    output.push_str(
        "\n## Limitations\n\n- Generated content may contain errors. Verify critical names, numbers, dates, conditions, decisions, ownership, and commitments against the source.\n- Inference and advice are not source facts.\n\n_Generated by AccordMesh._\n",
    );
    output
}

fn report_text(detail: &ProjectDetail, include_transcript: bool) -> String {
    let mut output = format!(
        "{}\n\nMeeting type: {}\nCreated: {}\nStatus: {}\n",
        detail.project.title,
        origin_label(detail.project.origin),
        detail.project.created_at,
        status_label(&detail.project.status)
    );

    append_report_artifacts_text(&mut output, detail);
    if include_transcript {
        append_transcript_text(&mut output, detail);
    }
    output.push_str(
        "\nLimitations\n\n- Generated content may contain errors. Verify critical names, numbers, dates, conditions, decisions, ownership, and commitments against the source.\n- Inference and advice are not source facts.\n\nGenerated by AccordMesh.\n",
    );
    output
}

fn append_report_artifacts_markdown(output: &mut String, detail: &ProjectDetail) {
    let artifacts = report_artifacts(detail);
    if artifacts.is_empty() {
        output.push_str("\n## Meeting report\n\nNo generated report sections were selected.\n");
        return;
    }
    for artifact in artifacts {
        output.push_str(&format!(
            "\n## {}\n\n{}\n",
            report_artifact_title(&artifact.artifact_type),
            report_payload_markdown(&artifact.artifact_type, &artifact.payload)
        ));
    }
}

fn append_report_artifacts_text(output: &mut String, detail: &ProjectDetail) {
    let artifacts = report_artifacts(detail);
    if artifacts.is_empty() {
        output.push_str("\nMeeting report\n\nNo generated report sections were selected.\n");
        return;
    }
    for artifact in artifacts {
        output.push_str(&format!(
            "\n{}\n\n{}\n",
            report_artifact_title(&artifact.artifact_type),
            report_payload_text(&artifact.artifact_type, &artifact.payload)
        ));
    }
}

fn report_artifacts(detail: &ProjectDetail) -> Vec<&AnalysisArtifact> {
    let mut selected = detail
        .artifacts
        .iter()
        .filter(|artifact| REPORT_ARTIFACT_ORDER.contains(&artifact.artifact_type.as_str()))
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| {
        report_artifact_rank(&left.artifact_type)
            .cmp(&report_artifact_rank(&right.artifact_type))
            .then_with(|| left.created_at.cmp(&right.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    selected
}

fn report_artifact_rank(artifact_type: &str) -> usize {
    REPORT_ARTIFACT_ORDER
        .iter()
        .position(|candidate| *candidate == artifact_type)
        .unwrap_or(REPORT_ARTIFACT_ORDER.len())
}

fn report_artifact_title(artifact_type: &str) -> &'static str {
    match artifact_type {
        "meeting_minutes" => "Meeting Minutes",
        "post_meeting_analysis" => "Post-meeting Analysis",
        "communication_review" => "Communication Review",
        "intelligent_comparison_report" => "Intelligent Comparison",
        _ => "Meeting Report",
    }
}

fn report_payload_markdown(artifact_type: &str, payload: &Value) -> String {
    match artifact_type {
        "meeting_minutes" => minutes_markdown(payload),
        "post_meeting_analysis" => ordered_object_markdown(
            payload,
            &[
                "overview",
                "majorTopics",
                "keyFacts",
                "confirmedDecisions",
                "conditions",
                "constraints",
                "unresolvedIssues",
                "ambiguities",
                "recommendedFollowUpActions",
                "uncertaintyNotes",
                "limitations",
            ],
        ),
        "communication_review" => {
            ordered_object_markdown(payload, &["observations", "improvedWording", "limitations"])
        }
        "intelligent_comparison_report" => ordered_object_markdown(
            payload,
            &[
                "overallAssessment",
                "correctlyCaptured",
                "missedOrIncomplete",
                "correctedInterpretations",
                "newlyDiscovered",
                "guidanceRevisions",
                "conclusionChanges",
                "recommendedFollowUps",
            ],
        ),
        _ => sanitized_markdown(payload),
    }
}

fn report_payload_text(artifact_type: &str, payload: &Value) -> String {
    match artifact_type {
        "meeting_minutes" => minutes_text(payload),
        "post_meeting_analysis" => ordered_object_text(
            payload,
            &[
                "overview",
                "majorTopics",
                "keyFacts",
                "confirmedDecisions",
                "conditions",
                "constraints",
                "unresolvedIssues",
                "ambiguities",
                "recommendedFollowUpActions",
                "uncertaintyNotes",
                "limitations",
            ],
        ),
        "communication_review" => {
            ordered_object_text(payload, &["observations", "improvedWording", "limitations"])
        }
        "intelligent_comparison_report" => ordered_object_text(
            payload,
            &[
                "overallAssessment",
                "correctlyCaptured",
                "missedOrIncomplete",
                "correctedInterpretations",
                "newlyDiscovered",
                "guidanceRevisions",
                "conclusionChanges",
                "recommendedFollowUps",
            ],
        ),
        _ => sanitized_text(payload),
    }
}

fn minutes_markdown(payload: &Value) -> String {
    let Some(object) = payload.as_object() else {
        return sanitized_markdown(payload);
    };
    let mut output = String::new();
    if let Some(sections) = object.get("sections").and_then(Value::as_array) {
        for section in sections {
            let title = section
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Section");
            let items = section.get("items").unwrap_or(&Value::Null);
            output.push_str(&format!("### {}\n\n{}\n\n", title, markdown_list(items)));
        }
    }
    if let Some(limitations) = object.get("limitations") {
        output.push_str(&format!(
            "### Minutes limitations\n\n{}\n",
            markdown_list(limitations)
        ));
    }
    if output.trim().is_empty() {
        sanitized_markdown(payload)
    } else {
        output.trim_end().to_string()
    }
}

fn minutes_text(payload: &Value) -> String {
    let Some(object) = payload.as_object() else {
        return sanitized_text(payload);
    };
    let mut output = String::new();
    if let Some(sections) = object.get("sections").and_then(Value::as_array) {
        for section in sections {
            let title = section
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Section");
            let items = section.get("items").unwrap_or(&Value::Null);
            output.push_str(&format!("{}\n{}\n\n", title, text_list(items)));
        }
    }
    if let Some(limitations) = object.get("limitations") {
        output.push_str(&format!(
            "Minutes limitations\n{}\n",
            text_list(limitations)
        ));
    }
    if output.trim().is_empty() {
        sanitized_text(payload)
    } else {
        output.trim_end().to_string()
    }
}

fn ordered_object_markdown(payload: &Value, keys: &[&str]) -> String {
    let Some(object) = payload.as_object() else {
        return sanitized_markdown(payload);
    };
    let mut sections = Vec::new();
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if is_empty_value(value) {
            sections.push(format!("### {}\n\nNone recorded.", human_type(key)));
        } else {
            sections.push(format!(
                "### {}\n\n{}",
                human_type(key),
                sanitized_markdown(value)
            ));
        }
    }
    if sections.is_empty() {
        sanitized_markdown(payload)
    } else {
        sections.join("\n\n")
    }
}

fn ordered_object_text(payload: &Value, keys: &[&str]) -> String {
    let Some(object) = payload.as_object() else {
        return sanitized_text(payload);
    };
    let mut sections = Vec::new();
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        let content = if is_empty_value(value) {
            "None recorded.".into()
        } else {
            sanitized_text(value)
        };
        sections.push(format!("{}\n{}", human_type(key), content));
    }
    if sections.is_empty() {
        sanitized_text(payload)
    } else {
        sections.join("\n\n")
    }
}

fn sanitized_markdown(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                return text.to_string();
            }
            let entries = map
                .iter()
                .filter(|(key, _)| !is_technical_key(key))
                .map(|(key, item)| format!("**{}:** {}", human_type(key), sanitized_markdown(item)))
                .collect::<Vec<_>>();
            if entries.is_empty() {
                "None recorded.".into()
            } else {
                entries.join("  \n")
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                "None recorded.".into()
            } else {
                items
                    .iter()
                    .map(|item| format!("- {}", sanitized_markdown(item)))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        Value::String(value) => value.clone(),
        Value::Null => "Not recorded.".into(),
        other => other.to_string(),
    }
}

fn sanitized_text(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                return text.to_string();
            }
            let entries = map
                .iter()
                .filter(|(key, _)| !is_technical_key(key))
                .map(|(key, item)| format!("{}: {}", human_type(key), sanitized_text(item)))
                .collect::<Vec<_>>();
            if entries.is_empty() {
                "None recorded.".into()
            } else {
                entries.join("\n")
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                "None recorded.".into()
            } else {
                items
                    .iter()
                    .map(|item| format!("- {}", sanitized_text(item)))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        Value::String(value) => value.clone(),
        Value::Null => "Not recorded.".into(),
        other => other.to_string(),
    }
}

fn markdown_list(value: &Value) -> String {
    match value {
        Value::Array(items) if items.is_empty() => "None recorded.".into(),
        Value::Array(items) => items
            .iter()
            .map(|item| format!("- {}", sanitized_markdown(item)))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => sanitized_markdown(value),
    }
}

fn text_list(value: &Value) -> String {
    match value {
        Value::Array(items) if items.is_empty() => "None recorded.".into(),
        Value::Array(items) => items
            .iter()
            .map(|item| format!("- {}", sanitized_text(item)))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => sanitized_text(value),
    }
}

fn is_empty_value(value: &Value) -> bool {
    matches!(value, Value::Null)
        || value.as_array().is_some_and(|items| items.is_empty())
        || value.as_object().is_some_and(|object| object.is_empty())
        || value.as_str().is_some_and(|text| text.trim().is_empty())
}

fn is_technical_key(key: &str) -> bool {
    matches!(
        key,
        "evidenceRefs"
            | "sourceArtifactIds"
            | "projectId"
            | "segmentId"
            | "sourceId"
            | "sourceIds"
            | "artifactId"
            | "generationRunId"
            | "providerId"
            | "modelId"
            | "promptVersion"
            | "schemaVersion"
            | "appVersion"
            | "createdAt"
    )
}

fn append_transcript_markdown(output: &mut String, detail: &ProjectDetail) {
    output.push_str("\n## Full Transcript\n");
    for (label, segments) in transcript_groups(detail) {
        if segments.is_empty() {
            continue;
        }
        output.push_str(&format!("\n### {}\n\n", label));
        let paragraph_numbers = paragraph_numbers(detail, &segments);
        for segment in segments {
            output.push_str(&format!(
                "- **{} · {}** {}\n",
                transcript_location(detail, segment, &paragraph_numbers),
                track_label(segment.track_role),
                segment.source_transcript
            ));
        }
    }
}

fn append_transcript_text(output: &mut String, detail: &ProjectDetail) {
    output.push_str("\nFull Transcript\n");
    for (label, segments) in transcript_groups(detail) {
        if segments.is_empty() {
            continue;
        }
        output.push_str(&format!("\n{}\n", label));
        let paragraph_numbers = paragraph_numbers(detail, &segments);
        for segment in segments {
            output.push_str(&format!(
                "{} · {}: {}\n",
                transcript_location(detail, segment, &paragraph_numbers),
                track_label(segment.track_role),
                segment.source_transcript
            ));
        }
    }
}

fn transcript_groups(detail: &ProjectDetail) -> Vec<(&'static str, Vec<&TimelineSegment>)> {
    let mut realtime = Vec::new();
    let mut uploaded = Vec::new();
    let mut unknown = Vec::new();
    for segment in &detail.timeline {
        match segment.track_role {
            TrackRole::UploadedMedia => uploaded.push(segment),
            TrackRole::RemoteSystemAudio
            | TrackRole::LocalMicrophone
            | TrackRole::RoomMicrophone => realtime.push(segment),
            TrackRole::Unknown => unknown.push(segment),
        }
    }
    let uploaded_label = if matches!(detail.project.origin, ProjectOrigin::UploadOnly) {
        "Uploaded source"
    } else {
        "Uploaded recording"
    };
    vec![
        ("Real-time capture", realtime),
        (uploaded_label, uploaded),
        ("Unknown source", unknown),
    ]
}

fn paragraph_numbers(
    detail: &ProjectDetail,
    segments: &[&TimelineSegment],
) -> HashMap<String, usize> {
    let mut counts = HashMap::<String, usize>::new();
    let mut numbers = HashMap::new();
    for segment in segments {
        if is_untimed_text_segment(detail, segment) {
            let count = counts.entry(segment.source_id.clone()).or_insert(0);
            *count += 1;
            numbers.insert(segment.id.clone(), *count);
        }
    }
    numbers
}

fn transcript_location(
    detail: &ProjectDetail,
    segment: &TimelineSegment,
    paragraph_numbers: &HashMap<String, usize>,
) -> String {
    if let Some(number) = paragraph_numbers.get(&segment.id) {
        let file_name = detail
            .media_assets
            .iter()
            .find(|asset| asset.id == segment.source_id)
            .map(|asset| asset.original_file_name.as_str())
            .unwrap_or("Text source");
        return format!("{} · Paragraph {}", file_name, number);
    }
    format!(
        "{}-{}",
        format_timestamp(segment.start_ms),
        format_timestamp(segment.end_ms)
    )
}

fn is_untimed_text_segment(detail: &ProjectDetail, segment: &TimelineSegment) -> bool {
    detail.media_assets.iter().any(|asset| {
        asset.id == segment.source_id
            && matches!(asset.kind, MediaKind::Transcript)
            && matches!(segment.track_role, TrackRole::UploadedMedia)
    })
}

fn format_timestamp(milliseconds: i64) -> String {
    let total_seconds = milliseconds.max(0) / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn track_label(track_role: TrackRole) -> &'static str {
    match track_role {
        TrackRole::RemoteSystemAudio => "Remote system audio",
        TrackRole::LocalMicrophone => "Local microphone",
        TrackRole::RoomMicrophone => "Room microphone",
        TrackRole::UploadedMedia => "Uploaded material",
        TrackRole::Unknown => "Unknown track",
    }
}

fn origin_label(origin: ProjectOrigin) -> &'static str {
    match origin {
        ProjectOrigin::RealtimeOnline => "Online real-time meeting",
        ProjectOrigin::RealtimeInPerson => "In-person real-time meeting",
        ProjectOrigin::UploadOnly => "Uploaded meeting material",
    }
}

fn status_label<T: std::fmt::Debug>(status: &T) -> String {
    human_type(&format!("{status:?}"))
}

fn json_export(detail: &ProjectDetail) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "formatVersion": "accordmesh-export-v1",
        "project": detail.project,
        "sourceTranscript": detail.timeline,
        "media": detail.media_assets,
        "artifacts": detail.artifacts,
        "generationRuns": detail.generation_runs,
        "limitations": [
            "Generated content may contain errors.",
            "Verify critical details against source evidence.",
            "Inference and advice are not source facts."
        ]
    }))
    .unwrap_or_else(|_| "{}".into())
}

fn human_artifact_value(artifact: &AnalysisArtifact, markdown: bool) -> String {
    if artifact.artifact_type == "literal_translation" {
        if let Some(text) = artifact
            .payload
            .get("translatedText")
            .and_then(Value::as_str)
        {
            return sanitize_legacy_translation_text(text);
        }
    }
    if markdown {
        human_value(&artifact.payload)
    } else {
        plain_value(&artifact.payload)
    }
}

fn sanitize_legacy_translation_text(value: &str) -> String {
    value
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let Some(first_end) = trimmed.find(']') else {
                return line.to_string();
            };
            if !trimmed.starts_with('[') {
                return line.to_string();
            }
            let timing = &trimmed[1..first_end];
            let timing_body = timing.strip_suffix(" ms").unwrap_or("");
            let mut parts = timing_body.split('-');
            let start = parts.next().unwrap_or("");
            let end = parts.next().unwrap_or("");
            if parts.next().is_some()
                || start.is_empty()
                || end.is_empty()
                || !start.chars().all(|ch| ch.is_ascii_digit())
                || !end.chars().all(|ch| ch.is_ascii_digit())
            {
                return line.to_string();
            }
            let remainder = trimmed[first_end + 1..].trim_start();
            if !remainder.starts_with('[') {
                return line.to_string();
            }
            let Some(second_end) = remainder.find(']') else {
                return line.to_string();
            };
            remainder[second_end + 1..].trim_start().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod translation_readability_tests {
    use super::sanitize_legacy_translation_text;

    #[test]
    fn controlled_regeneration_legacy_translation_metadata_prefixes_are_removed_without_touching_plain_text(
    ) {
        let value = "[57-20067 ms][LocalMicrophone] AccordMesh local acceptance transcript 13.\nPlain second line.";
        assert_eq!(
            sanitize_legacy_translation_text(value),
            "AccordMesh local acceptance transcript 13.\nPlain second line."
        );
        assert_eq!(
            sanitize_legacy_translation_text("[Agenda] Keep this bracketed heading."),
            "[Agenda] Keep this bracketed heading."
        );
    }
}

fn legacy_markdown(detail: &ProjectDetail) -> String {
    let mut output = format!(
        "# {}\n\n- Origin: {:?}\n- Created: {}\n- Updated: {}\n\n## Source transcript\n\n",
        detail.project.title,
        detail.project.origin,
        detail.project.created_at,
        detail.project.updated_at
    );
    for segment in &detail.timeline {
        output.push_str(&format!(
            "- **{}-{} ms / {:?}** {}\n",
            segment.start_ms, segment.end_ms, segment.track_role, segment.source_transcript
        ));
    }
    for artifact in &detail.artifacts {
        output.push_str(&format!(
            "\n## {}\n\n{}\n\n_Provenance: provider {}, model {}, prompt {}, schema {}, generated {}._\n",
            human_type(&artifact.artifact_type),
            human_artifact_value(artifact, true),
            artifact.provider_id,
            artifact.model_id,
            artifact.prompt_version,
            artifact.schema_version,
            artifact.created_at
        ));
    }
    output.push_str(
        "\n## Limitations\n\n- Generated content may contain errors. Verify critical names, numbers, dates, conditions, decisions, ownership, and commitments against the source.\n- Inference and advice are not source facts.\n",
    );
    output
}

fn legacy_text(detail: &ProjectDetail) -> String {
    let mut output = format!(
        "{}\n\nOrigin: {:?}\nCreated: {}\nUpdated: {}\n\nSource transcript\n\n",
        detail.project.title,
        detail.project.origin,
        detail.project.created_at,
        detail.project.updated_at
    );
    for segment in &detail.timeline {
        output.push_str(&format!(
            "{}-{} ms / {:?}: {}\n",
            segment.start_ms, segment.end_ms, segment.track_role, segment.source_transcript
        ));
    }
    for artifact in &detail.artifacts {
        output.push_str(&format!(
            "\n{}\n\n{}\n\nProvenance: provider {}, model {}, prompt {}, schema {}, generated {}.\n",
            human_type(&artifact.artifact_type),
            human_artifact_value(artifact, false),
            artifact.provider_id,
            artifact.model_id,
            artifact.prompt_version,
            artifact.schema_version,
            artifact.created_at
        ));
    }
    output.push_str(
        "\nLimitations\n\n- Generated content may contain errors. Verify critical names, numbers, dates, conditions, decisions, ownership, and commitments against the source.\n- Inference and advice are not source facts.\n",
    );
    output
}

fn human_type(value: &str) -> String {
    let mut words = Vec::new();
    let mut current = String::new();
    for character in value.chars() {
        if character == '_' || character == '-' {
            if !current.is_empty() {
                words.push(current);
                current = String::new();
            }
            continue;
        }
        if character.is_uppercase() && !current.is_empty() {
            words.push(current);
            current = String::new();
        }
        current.push(character);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
        .into_iter()
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn human_value(value: &Value) -> String {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| format!("### {}\n\n{}", human_type(key), human_value(value)))
            .collect::<Vec<_>>()
            .join("\n\n"),
        Value::Array(items) => {
            if items.is_empty() {
                "None recorded.".into()
            } else {
                items
                    .iter()
                    .map(|item| format!("- {}", human_value(item)))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        Value::String(value) => value.clone(),
        Value::Null => "Not recorded.".into(),
        other => other.to_string(),
    }
}

fn plain_value(value: &Value) -> String {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| {
                let rendered = plain_value(value);
                if matches!(value, Value::Object(_) | Value::Array(_)) {
                    format!("{}:\n{}", human_type(key), indent(&rendered, 2))
                } else {
                    format!("{}: {}", human_type(key), rendered)
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
        Value::Array(items) => {
            if items.is_empty() {
                "None recorded.".into()
            } else {
                items
                    .iter()
                    .map(|item| {
                        let rendered = plain_value(item);
                        let mut lines = rendered.lines();
                        match lines.next() {
                            Some(first) => {
                                let rest = lines
                                    .map(|line| format!("  {line}"))
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                if rest.is_empty() {
                                    format!("- {first}")
                                } else {
                                    format!("- {first}\n{rest}")
                                }
                            }
                            None => "-".into(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        Value::String(value) => value.clone(),
        Value::Null => "Not recorded.".into(),
        other => other.to_string(),
    }
}

fn indent(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}
