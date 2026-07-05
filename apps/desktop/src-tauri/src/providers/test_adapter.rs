use crate::providers::registry::full_capabilities;
use crate::providers::{
    ProviderDefinition, ProviderField, ProviderModelAssignment, ProviderResult,
};

pub const ID: &str = "test_adapter";
pub const UI_ONLY_ERROR: &str = "ERR_TEST_PROVIDER_ADAPTER_UI_ONLY";

pub fn definition() -> ProviderDefinition {
    let mut capabilities = full_capabilities(&["ui-only"]);
    capabilities.supports_streaming = false;
    ProviderDefinition {
        id: ID.into(),
        display_name_key: "providers.testAdapter.displayName".into(),
        credential_schema: vec![],
        configuration_schema: vec![
            model_field("fileTranscriptionModel", "test-transcribe-ui-v1"),
            model_field("realtimeTranscriptionModel", "test-realtime-ui-v1"),
            model_field("textTranslationModel", "test-translate-ui-v1"),
            model_field("segmentUnderstandingModel", "test-segment-ui-v1"),
            model_field("meetingSynthesisModel", "test-synthesis-ui-v1"),
            model_field("communicationReviewModel", "test-review-ui-v1"),
            model_field("comparisonReportModel", "test-comparison-ui-v1"),
            model_field("meetingMinutesModel", "test-minutes-ui-v1"),
        ],
        model_assignments: vec![
            assignment("fileTranscription", "fileTranscriptionModel"),
            assignment("realtimeTranscription", "realtimeTranscriptionModel"),
            assignment("textTranslation", "textTranslationModel"),
            assignment("segmentUnderstanding", "segmentUnderstandingModel"),
            assignment("meetingSynthesis", "meetingSynthesisModel"),
            assignment("communicationReview", "communicationReviewModel"),
            assignment("comparisonReport", "comparisonReportModel"),
            assignment("meetingMinutes", "meetingMinutesModel"),
        ],
        capabilities,
    }
}

pub fn validate_configuration(_: &serde_json::Value) -> ProviderResult<()> {
    Err(UI_ONLY_ERROR)
}

fn model_field(id: &str, default_value: &str) -> ProviderField {
    ProviderField {
        id: id.into(),
        label_key: format!("providers.fields.{id}"),
        field_type: "text".into(),
        required: false,
        secret: false,
        default_value: Some(default_value.into()),
    }
}

fn assignment(capability: &str, configuration_field_id: &str) -> ProviderModelAssignment {
    ProviderModelAssignment {
        capability: capability.into(),
        configuration_field_id: configuration_field_id.into(),
    }
}
