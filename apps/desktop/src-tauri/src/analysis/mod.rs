use jsonschema::JSONSchema;

pub fn validate_artifact(
    artifact_type: &str,
    payload: &serde_json::Value,
) -> Result<(), &'static str> {
    let raw = match artifact_type {
        "literal_translation" => {
            include_str!("../../../../../packages/schemas/literal-translation.schema.json")
        }
        "segment_understanding" => {
            include_str!("../../../../../packages/schemas/segment-understanding.schema.json")
        }
        "post_meeting_analysis" => {
            include_str!("../../../../../packages/schemas/post-meeting-analysis.schema.json")
        }
        "communication_review" => {
            include_str!("../../../../../packages/schemas/communication-review.schema.json")
        }
        "intelligent_comparison_report" => {
            include_str!("../../../../../packages/schemas/intelligent-comparison.schema.json")
        }
        "meeting_minutes" => {
            include_str!("../../../../../packages/schemas/meeting-minutes.schema.json")
        }
        _ => return Err("ERR_PROVIDER_SCHEMA"),
    };
    let schema: serde_json::Value = serde_json::from_str(raw).map_err(|_| "ERR_PROVIDER_SCHEMA")?;
    let validator = JSONSchema::compile(&schema).map_err(|_| "ERR_PROVIDER_SCHEMA")?;
    if validator.is_valid(payload) {
        Ok(())
    } else {
        Err("ERR_PROVIDER_SCHEMA")
    }
}
