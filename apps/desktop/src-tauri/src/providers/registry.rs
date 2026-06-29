use std::sync::Arc;

use crate::storage::repository::Repository;

use super::mock::MockProvider;
use super::openai::{self, OpenAiProvider};
use super::{Provider, ProviderCapabilities, ProviderDefinition};

pub fn definitions() -> Vec<ProviderDefinition> {
    vec![MockProvider::definition(), openai::definition()]
}

pub const SUPPORTED_LANGUAGE_CODES: &[&str] = &[
    "af", "ar", "hy", "az", "be", "bs", "bg", "ca", "zh-Hans", "zh-Hant", "hr", "cs", "da", "nl",
    "en", "et", "fi", "fr", "gl", "de", "el", "he", "hi", "hu", "is", "id", "it", "ja", "kn", "kk",
    "ko", "lv", "lt", "mk", "ms", "mr", "mi", "ne", "no", "fa", "pl", "pt", "ro", "ru", "sr", "sk",
    "sl", "es", "sw", "sv", "tl", "ta", "th", "tr", "uk", "ur", "vi", "cy",
];

pub fn validate_language_contract(
    capabilities: &ProviderCapabilities,
    source_language: Option<&str>,
    translation_target_language: Option<&str>,
    output_languages: &[&str],
) -> Result<(), &'static str> {
    let source_supported = match source_language {
        Some(language) => capabilities
            .supported_source_languages
            .iter()
            .any(|value| value.as_str() == language),
        None => {
            capabilities.supports_language_auto_detection
                && capabilities
                    .supported_source_languages
                    .iter()
                    .any(|value| value == "auto")
        }
    };
    if !source_supported {
        return Err("ERR_SOURCE_LANGUAGE_UNSUPPORTED");
    }
    if let Some(language) = translation_target_language {
        if !capabilities.text_translation
            || !capabilities
                .supported_target_languages
                .iter()
                .any(|value| value.as_str() == language)
        {
            return Err("ERR_TARGET_LANGUAGE_UNSUPPORTED");
        }
    }
    if output_languages.iter().any(|language| {
        !capabilities
            .supported_target_languages
            .iter()
            .any(|value| value.as_str() == *language)
    }) {
        return Err("ERR_TARGET_LANGUAGE_UNSUPPORTED");
    }
    Ok(())
}

pub fn resolve(
    provider_id: &str,
    repo: &Repository,
    master_key: &[u8],
) -> Result<Arc<dyn Provider>, &'static str> {
    match provider_id {
        "mock" => {
            let config = repo
                .provider_configuration("mock", master_key)
                .map_err(|_| "ERR_PROVIDER_CONFIG")?
                .unwrap_or_default();
            Ok(Arc::new(MockProvider::from_configuration(&config)))
        }
        "openai" => {
            let config = repo
                .provider_configuration("openai", master_key)
                .map_err(|_| "ERR_PROVIDER_CONFIG")?
                .ok_or("ERR_PROVIDER_NOT_CONFIGURED")?;
            Ok(Arc::new(OpenAiProvider::from_configuration(config)?))
        }
        _ => Err("ERR_PROVIDER_NOT_FOUND"),
    }
}

pub fn validate_configuration(
    provider_id: &str,
    value: &serde_json::Value,
) -> Result<(), &'static str> {
    match provider_id {
        "mock" => {
            let scenario = value
                .get("scenario")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("normal");
            if [
                "normal",
                "timeout",
                "quota",
                "authentication",
                "unavailable",
                "unsupported",
            ]
            .contains(&scenario)
            {
                Ok(())
            } else {
                Err("ERR_PROVIDER_CONFIG")
            }
        }
        "openai" => openai::validate_configuration(value),
        _ => Err("ERR_PROVIDER_NOT_FOUND"),
    }
}

pub fn require(capabilities: &ProviderCapabilities, names: &[&str]) -> Result<(), &'static str> {
    let supported = |name: &str| match name {
        "file_transcription" => capabilities.file_transcription,
        "realtime_transcription" => capabilities.realtime_transcription,
        "text_translation" => capabilities.text_translation,
        "segment_understanding" => capabilities.segment_understanding,
        "meeting_synthesis" => capabilities.meeting_synthesis,
        "communication_review" => capabilities.communication_review,
        "comparison_report" => capabilities.comparison_report,
        "meeting_minutes" => capabilities.meeting_minutes,
        _ => false,
    };
    if names.iter().all(|name| supported(name)) {
        Ok(())
    } else {
        Err("ERR_PROVIDER_UNSUPPORTED_CAPABILITY")
    }
}

pub fn full_capabilities(formats: &[&str]) -> ProviderCapabilities {
    let supported_languages = SUPPORTED_LANGUAGE_CODES
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    let mut source_languages = vec!["auto".to_string()];
    source_languages.extend(supported_languages.iter().cloned());
    ProviderCapabilities {
        file_transcription: true,
        realtime_transcription: true,
        text_translation: true,
        segment_understanding: true,
        meeting_synthesis: true,
        communication_review: true,
        comparison_report: true,
        meeting_minutes: true,
        supports_streaming: true,
        supports_structured_output: true,
        supports_language_auto_detection: true,
        supports_code_switching: true,
        supported_input_formats: formats.iter().map(|v| (*v).into()).collect(),
        supported_source_languages: source_languages,
        supported_target_languages: supported_languages,
    }
}
