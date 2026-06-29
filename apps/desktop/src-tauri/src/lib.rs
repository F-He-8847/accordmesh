mod analysis;
mod audio;
mod auth;
mod comparison;
mod context;
mod crypto;
mod export;
mod jobs;
mod media;
mod platform;
mod projects;
mod providers;
#[cfg(test)]
mod public_contract_tests;
mod realtime;
mod reset;
mod storage;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use auth::{create_vault_record, load_vault_record, unlock_master_key};
use jobs::{QueuedFile, UploadJobPayload};
use projects::types::*;
use serde::{Deserialize, Serialize};
use storage::repository::{DeleteProjectError, Repository};
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("{0}")]
    Stable(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Crypto(#[from] crypto::CryptoError),
}
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let code = match self {
            Self::Stable(code) => *code,
            Self::Io(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                "ERR_VAULT_ALREADY_EXISTS"
            }
            Self::Io(_) => "ERR_IO",
            Self::Sql(_) => "ERR_STORAGE",
            Self::Json(_) => "ERR_JSON",
            Self::Crypto(crypto::CryptoError::Corrupt) => "ERR_ENCRYPTED_DATA_CORRUPT",
            Self::Crypto(crypto::CryptoError::UnsupportedVersion) => "ERR_ENCRYPTED_DATA_VERSION",
            Self::Crypto(_) => "ERR_CRYPTO",
        };
        serializer.serialize_str(code)
    }
}
impl From<Box<dyn std::error::Error>> for AppError {
    fn from(_: Box<dyn std::error::Error>) -> Self {
        Self::Stable("ERR_INTERNAL")
    }
}
impl From<&'static str> for AppError {
    fn from(value: &'static str) -> Self {
        Self::Stable(value)
    }
}

struct AppCoreState {
    data_dir: PathBuf,
    master_key: Option<Zeroizing<Vec<u8>>>,
    selections: media::SelectionRegistry,
    job_runtimes: jobs::RuntimeRegistry,
    realtime_sessions: HashMap<String, realtime::ActiveRealtime>,
    locking: bool,
    resetting: bool,
    operations_in_flight: usize,
}

impl Default for AppCoreState {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::new(),
            master_key: None,
            selections: HashMap::new(),
            job_runtimes: jobs::runtime_registry(),
            realtime_sessions: HashMap::new(),
            locking: false,
            resetting: false,
            operations_in_flight: 0,
        }
    }
}

impl AppCoreState {
    fn repo(&self) -> Result<Repository, AppError> {
        Ok(Repository::new(self.data_dir.clone())?)
    }
    fn key(&self) -> Result<&[u8], AppError> {
        if self.locking {
            return Err(AppError::Stable("ERR_LOCKED"));
        }
        self.master_key
            .as_deref()
            .map(|v| v.as_slice())
            .ok_or(AppError::Stable("ERR_LOCKED"))
    }
    fn cloned_key(&self) -> Result<Zeroizing<Vec<u8>>, AppError> {
        if self.locking {
            return Err(AppError::Stable("ERR_LOCKED"));
        }
        self.master_key
            .clone()
            .ok_or(AppError::Stable("ERR_LOCKED"))
    }
    fn clear_sensitive_memory(&mut self) {
        self.selections.clear();
        if let Some(mut key) = self.master_key.take() {
            key.zeroize();
        }
    }
}

impl Drop for AppCoreState {
    fn drop(&mut self) {
        self.clear_sensitive_memory();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupStatus {
    vault_exists: bool,
    unlocked: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResetVaultStatus {
    active_realtime_sessions: usize,
    cleanup_pending_sessions: usize,
    active_jobs: usize,
    operations_in_flight: usize,
    reset_in_progress: bool,
    recovery_required: bool,
    can_start: bool,
    active_work_blocks_reset: bool,
}

fn setup_status_from_state(state: &AppCoreState) -> SetupStatus {
    SetupStatus {
        vault_exists: state.data_dir.join("vault/vault.json").exists(),
        unlocked: state.master_key.is_some() && !state.locking,
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderConfigurationStatus {
    provider_id: String,
    stored: bool,
    configured: bool,
    configured_fields: Vec<String>,
    credential_fields_configured: Vec<String>,
    missing_required_fields: Vec<String>,
    configuration: serde_json::Map<String, serde_json::Value>,
    masked_summary: String,
    updated_at: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderCredentialInput {
    provider_id: String,
    fields: serde_json::Value,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RealtimeStartInput {
    mode: RealtimeMode,
    title: Option<String>,
    device_id: String,
    source_language: Option<String>,
    translation_target_language: Option<String>,
    analysis_output_language: String,
    provider_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadInput {
    title: Option<String>,
    files: Vec<UploadFileInput>,
    source_language: Option<String>,
    translation_target_language: Option<String>,
    analysis_output_language: String,
    minutes_output_language: String,
    provider_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachInput {
    project_id: String,
    files: Vec<UploadFileInput>,
    source_language: Option<String>,
    translation_target_language: Option<String>,
    analysis_output_language: String,
    minutes_output_language: String,
    provider_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadFileInput {
    selection_token: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegenerateInput {
    request_id: String,
    project_id: String,
    artifact_type: String,
    provider_id: String,
    #[serde(default)]
    model_id: Option<String>,
    output_language: String,
    #[serde(default)]
    source_segment_ids: Vec<String>,
    #[serde(default)]
    source_artifact_ids: Vec<String>,
}

#[tauri::command]
fn setup_status(state: State<Mutex<AppCoreState>>) -> Result<SetupStatus, AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    Ok(setup_status_from_state(&state))
}

#[tauri::command]
fn create_vault(
    password: String,
    app: AppHandle,
    state: State<Mutex<AppCoreState>>,
) -> Result<SetupStatus, AppError> {
    if password.len() < 8 {
        return Err(AppError::Stable("ERR_PASSWORD_TOO_SHORT"));
    }
    let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    if state.locking || reset::recovery_pending(&state.data_dir) {
        return Err(AppError::Stable("ERR_RESET_RECOVERY_REQUIRED"));
    }
    if state.data_dir.join("vault/vault.json").exists() {
        return Err(AppError::Stable("ERR_VAULT_ALREADY_EXISTS"));
    }
    let repo = state.repo()?;
    if repo.has_user_data()? {
        return Err(AppError::Stable("ERR_ORPHANED_DATA"));
    }
    let master = create_vault_record(&state.data_dir, &password)?;
    repo.initialize()?;
    let mock = serde_json::json!({"scenario":"normal"});
    repo.upsert_provider_configuration(
        "mock",
        &crypto::seal(&master, &serde_json::to_vec(&mock)?)?,
        &["scenario".into()],
    )?;
    state.master_key = Some(master);
    app.emit(
        "accordmesh://vault-state",
        serde_json::json!({"unlocked":true,"vaultExists":true}),
    )
    .ok();
    Ok(SetupStatus {
        vault_exists: true,
        unlocked: true,
    })
}

#[tauri::command]
fn unlock(
    password: String,
    app: AppHandle,
    state: State<Mutex<AppCoreState>>,
) -> Result<SetupStatus, AppError> {
    let (repo, master, resumable, registry);
    {
        let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        if state.locking || reset::recovery_pending(&state.data_dir) {
            return Err(AppError::Stable("ERR_RESET_RECOVERY_REQUIRED"));
        }
        let record = load_vault_record(&state.data_dir)?;
        master = unlock_master_key(&record, &password)
            .map_err(|_| AppError::Stable("ERR_INVALID_PASSWORD"))?;
        repo = state.repo()?;
        repo.ensure_legacy_keys(&master)?;
        realtime::recover_spool_jobs(&repo)?;
        resumable = repo.resumable_job_ids()?;
        state.master_key = Some(master.clone());
        state.locking = true;
        registry = state.job_runtimes.clone();
    }
    for id in resumable {
        if let Err(code) = jobs::spawn_recovered(
            app.clone(),
            repo.clone(),
            master.clone(),
            id,
            registry.clone(),
        ) {
            if let Ok(mut state) = state.lock() {
                state.clear_sensitive_memory();
                state.locking = false;
            }
            return Err(AppError::Stable(code));
        }
    }
    {
        let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        state.locking = false;
    }
    app.emit(
        "accordmesh://vault-state",
        serde_json::json!({"unlocked":true,"vaultExists":true}),
    )
    .ok();
    Ok(SetupStatus {
        vault_exists: true,
        unlocked: true,
    })
}

#[tauri::command]
fn lock(app: AppHandle, state: State<Mutex<AppCoreState>>) -> Result<SetupStatus, AppError> {
    let vault_exists;
    {
        let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        state
            .realtime_sessions
            .retain(|_, runtime| !runtime.is_completed());
        if state
            .realtime_sessions
            .values()
            .any(|runtime| runtime.is_active())
        {
            return Err(AppError::Stable("ERR_ACTIVE_SESSION"));
        }
        if state
            .realtime_sessions
            .values()
            .any(|runtime| runtime.cleanup_pending())
        {
            return Err(AppError::Stable("ERR_REALTIME_CLEANUP_BUSY"));
        }
        if state.operations_in_flight > 0 {
            return Err(AppError::Stable("ERR_LOCK_BUSY"));
        }
        state.key()?;
        if jobs::has_active_runtimes(&state.job_runtimes) {
            return Err(AppError::Stable("ERR_LOCK_ACTIVE_JOB"));
        }
        state.locking = true;
        vault_exists = state.data_dir.join("vault/vault.json").exists();
        state.clear_sensitive_memory();
        state.locking = false;
    }
    if let Some(window) = app.get_webview_window("overlay") {
        window.hide().ok();
    }
    app.emit(
        "accordmesh://vault-state",
        serde_json::json!({"unlocked":false,"vaultExists":vault_exists}),
    )
    .ok();
    Ok(SetupStatus {
        vault_exists,
        unlocked: false,
    })
}

fn reset_vault_status_from_state(state: &AppCoreState) -> ResetVaultStatus {
    let active_realtime_sessions = state
        .realtime_sessions
        .values()
        .filter(|runtime| runtime.is_active())
        .count();
    let cleanup_pending_sessions = state
        .realtime_sessions
        .values()
        .filter(|runtime| runtime.cleanup_pending())
        .count();
    let active_jobs = jobs::active_runtime_count(&state.job_runtimes);
    let recovery_required = reset::recovery_pending(&state.data_dir);
    let active_work_blocks_reset =
        active_realtime_sessions + cleanup_pending_sessions + active_jobs > 0
            || state.operations_in_flight > 0;
    ResetVaultStatus {
        active_realtime_sessions,
        cleanup_pending_sessions,
        active_jobs,
        operations_in_flight: state.operations_in_flight,
        reset_in_progress: state.resetting,
        recovery_required,
        can_start: !state.locking
            && !state.resetting
            && !recovery_required
            && !active_work_blocks_reset,
        active_work_blocks_reset,
    }
}

#[tauri::command]
fn reset_vault_status(state: State<Mutex<AppCoreState>>) -> Result<ResetVaultStatus, AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    Ok(reset_vault_status_from_state(&state))
}

#[tauri::command]
fn reset_vault(
    confirmation: String,
    app: AppHandle,
    state: State<Mutex<AppCoreState>>,
) -> Result<SetupStatus, AppError> {
    if confirmation != "RESET" {
        return Err(AppError::Stable("ERR_RESET_CONFIRMATION"));
    }
    let data_dir;
    {
        let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        let reset_status = reset_vault_status_from_state(&state);
        if reset_status.recovery_required {
            return Err(AppError::Stable("ERR_RESET_RECOVERY_REQUIRED"));
        }
        if state.locking || reset_status.reset_in_progress || reset_status.active_work_blocks_reset
        {
            return Err(AppError::Stable("ERR_RESET_BUSY"));
        }
        state.resetting = true;
        state.locking = true;
        data_dir = state.data_dir.clone();
        state.clear_sensitive_memory();
    }

    match reset::atomic_reset_data_dir(&data_dir) {
        Ok(()) => {
            let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
            state.clear_sensitive_memory();
            state.job_runtimes = jobs::runtime_registry();
            state.realtime_sessions.clear();
            state.operations_in_flight = 0;
            state.resetting = false;
            state.locking = false;
        }
        Err(reset::ResetError::Preserved) => {
            if let Ok(mut state) = state.lock() {
                state.resetting = false;
                state.locking = false;
                state.clear_sensitive_memory();
            }
            if let Some(window) = app.get_webview_window("overlay") {
                window.hide().ok();
            }
            let vault_exists = data_dir.join("vault/vault.json").exists();
            app.emit("accordmesh://vault-state",serde_json::json!({"unlocked":false,"vaultExists":vault_exists,"errorCode":"ERR_RESET_FAILED_PRESERVED"})).ok();
            return Err(AppError::Stable("ERR_RESET_FAILED_PRESERVED"));
        }
        Err(reset::ResetError::RecoveryRequired) => {
            if let Ok(mut state) = state.lock() {
                state.resetting = false;
                state.locking = true;
                state.clear_sensitive_memory();
            }
            if let Some(window) = app.get_webview_window("overlay") {
                window.hide().ok();
            }
            let vault_exists = data_dir.join("vault/vault.json").exists();
            app.emit("accordmesh://vault-state",serde_json::json!({"unlocked":false,"vaultExists":vault_exists,"errorCode":"ERR_RESET_RECOVERY_REQUIRED"})).ok();
            return Err(AppError::Stable("ERR_RESET_RECOVERY_REQUIRED"));
        }
    }

    if let Some(window) = app.get_webview_window("overlay") {
        window.hide().ok();
    }
    app.emit(
        "accordmesh://vault-state",
        serde_json::json!({"unlocked":false,"vaultExists":false}),
    )
    .ok();
    app.emit(
        "accordmesh://vault-reset",
        serde_json::json!({"completed":true}),
    )
    .ok();
    Ok(SetupStatus {
        vault_exists: false,
        unlocked: false,
    })
}

fn provider_field<'a>(
    definition: &'a providers::ProviderDefinition,
    id: &str,
) -> Option<&'a providers::ProviderField> {
    definition
        .credential_schema
        .iter()
        .chain(definition.configuration_schema.iter())
        .find(|field| field.id == id)
}

fn normalized_provider_value(
    field: &providers::ProviderField,
    value: &serde_json::Value,
) -> Result<Option<serde_json::Value>, AppError> {
    if value.is_null() {
        return Ok(None);
    }
    match field.field_type.as_str() {
        "text" | "password" | "select" => {
            let text = value
                .as_str()
                .ok_or(AppError::Stable("ERR_PROVIDER_CONFIG"))?
                .trim();
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(serde_json::Value::String(text.to_owned())))
            }
        }
        "boolean" => value
            .as_bool()
            .map(|_| Some(value.clone()))
            .ok_or(AppError::Stable("ERR_PROVIDER_CONFIG")),
        "number" => value
            .as_f64()
            .map(|_| Some(value.clone()))
            .ok_or(AppError::Stable("ERR_PROVIDER_CONFIG")),
        _ => Err(AppError::Stable("ERR_PROVIDER_CONFIG")),
    }
}

fn merge_provider_configuration(
    definition: &providers::ProviderDefinition,
    existing: Option<&serde_json::Value>,
    incoming: &serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let incoming = incoming
        .as_object()
        .ok_or(AppError::Stable("ERR_PROVIDER_CONFIG"))?;
    let mut merged = serde_json::Map::new();
    if let Some(existing) = existing.and_then(serde_json::Value::as_object) {
        for (key, value) in existing {
            if provider_field(definition, key).is_some() {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    for (key, value) in incoming {
        let field =
            provider_field(definition, key).ok_or(AppError::Stable("ERR_PROVIDER_CONFIG"))?;
        match normalized_provider_value(field, value)? {
            Some(value) => {
                merged.insert(key.clone(), value);
            }
            None if field.secret => {}
            None => {
                merged.remove(key);
            }
        }
    }
    let present = |id: &str| {
        merged.get(id).is_some_and(|value| match value {
            serde_json::Value::String(text) => !text.trim().is_empty(),
            serde_json::Value::Null => false,
            _ => true,
        })
    };
    if definition
        .credential_schema
        .iter()
        .chain(definition.configuration_schema.iter())
        .any(|field| field.required && !present(&field.id))
    {
        return Err(AppError::Stable("ERR_PROVIDER_CONFIG"));
    }
    let merged = serde_json::Value::Object(merged);
    providers::registry::validate_configuration(&definition.id, &merged)
        .map_err(AppError::Stable)?;
    Ok(merged)
}

fn safe_provider_configuration(
    definition: &providers::ProviderDefinition,
    raw: Option<&serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut safe = serde_json::Map::new();
    if let Some(raw) = raw.and_then(serde_json::Value::as_object) {
        for field in definition
            .credential_schema
            .iter()
            .chain(definition.configuration_schema.iter())
        {
            if !field.secret {
                if let Some(value) = raw.get(&field.id) {
                    safe.insert(field.id.clone(), value.clone());
                }
            }
        }
    }
    safe
}

fn provider_value_present(raw: Option<&serde_json::Value>, id: &str) -> bool {
    raw.and_then(serde_json::Value::as_object)
        .and_then(|value| value.get(id))
        .is_some_and(|value| match value {
            serde_json::Value::String(text) => !text.trim().is_empty(),
            serde_json::Value::Null => false,
            _ => true,
        })
}

fn provider_readiness(
    definition: &providers::ProviderDefinition,
    raw: Option<&serde_json::Value>,
) -> (bool, Vec<String>, Vec<String>, String) {
    let credential_fields_configured = definition
        .credential_schema
        .iter()
        .filter(|field| field.secret && provider_value_present(raw, &field.id))
        .map(|field| field.id.clone())
        .collect::<Vec<_>>();
    let missing_required_fields = definition
        .credential_schema
        .iter()
        .chain(definition.configuration_schema.iter())
        .filter(|field| field.required && !provider_value_present(raw, &field.id))
        .map(|field| field.id.clone())
        .collect::<Vec<_>>();
    if raw.is_none() {
        return (
            false,
            credential_fields_configured,
            missing_required_fields,
            "not_configured".into(),
        );
    }
    if !missing_required_fields.is_empty() {
        let missing_secret = definition
            .credential_schema
            .iter()
            .any(|field| field.secret && missing_required_fields.contains(&field.id));
        return (
            false,
            credential_fields_configured,
            missing_required_fields,
            if missing_secret {
                "credential_missing"
            } else {
                "configuration_incomplete"
            }
            .into(),
        );
    }
    let ready = raw.is_some_and(|value| {
        providers::registry::validate_configuration(&definition.id, value).is_ok()
    });
    let summary = if ready {
        "ready"
    } else {
        "configuration_invalid"
    };
    (
        ready,
        credential_fields_configured,
        missing_required_fields,
        summary.into(),
    )
}

fn remove_provider_secret_value(
    definition: &providers::ProviderDefinition,
    raw: Option<&serde_json::Value>,
    field_id: &str,
) -> Result<Option<serde_json::Value>, AppError> {
    let field =
        provider_field(definition, field_id).ok_or(AppError::Stable("ERR_PROVIDER_CONFIG"))?;
    if !field.secret {
        return Err(AppError::Stable("ERR_PROVIDER_CONFIG"));
    }
    let Some(raw) = raw else {
        return Ok(None);
    };
    let mut object = raw
        .as_object()
        .cloned()
        .ok_or(AppError::Stable("ERR_PROVIDER_CONFIG"))?;
    object.remove(field_id);
    if object.is_empty() {
        Ok(None)
    } else {
        Ok(Some(serde_json::Value::Object(object)))
    }
}

fn provider_configuration_statuses(
    repo: &Repository,
    master_key: &[u8],
) -> Result<Vec<ProviderConfigurationStatus>, AppError> {
    let configured = repo.provider_statuses()?;
    let mut by_id = configured
        .into_iter()
        .map(|(id, fields, updated)| (id, (fields, updated)))
        .collect::<HashMap<_, _>>();
    let mut statuses = Vec::new();
    for definition in providers::registry::definitions() {
        let current = by_id.remove(&definition.id);
        let raw = if current.is_some() {
            repo.provider_configuration(&definition.id, master_key)?
        } else {
            None
        };
        let (ready, credential_fields_configured, missing_required_fields, masked_summary) =
            provider_readiness(&definition, raw.as_ref());
        statuses.push(ProviderConfigurationStatus {
            provider_id: definition.id.clone(),
            stored: current.is_some(),
            configured: ready,
            configured_fields: current
                .as_ref()
                .map(|value| value.0.clone())
                .unwrap_or_default(),
            credential_fields_configured,
            missing_required_fields,
            configuration: safe_provider_configuration(&definition, raw.as_ref()),
            masked_summary,
            updated_at: current.map(|value| value.1),
        });
    }
    Ok(statuses)
}

#[tauri::command]
fn provider_definitions() -> Vec<providers::ProviderDefinition> {
    providers::registry::definitions()
}
#[tauri::command]
fn provider_configuration_status(
    state: State<Mutex<AppCoreState>>,
) -> Result<Vec<ProviderConfigurationStatus>, AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    let master_key = state.key()?;
    provider_configuration_statuses(&state.repo()?, master_key)
}
#[tauri::command]
fn save_provider_credentials(
    input: ProviderCredentialInput,
    state: State<Mutex<AppCoreState>>,
) -> Result<(), AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    let definition = providers::registry::definitions()
        .into_iter()
        .find(|value| value.id == input.provider_id)
        .ok_or(AppError::Stable("ERR_PROVIDER_NOT_FOUND"))?;
    let repo = state.repo()?;
    let existing = repo.provider_configuration(&input.provider_id, state.key()?)?;
    let merged = merge_provider_configuration(&definition, existing.as_ref(), &input.fields)?;
    let mut fields = merged
        .as_object()
        .expect("merged provider configuration must be an object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    fields.sort();
    let sealed = crypto::seal(state.key()?, &serde_json::to_vec(&merged)?)?;
    repo.upsert_provider_configuration(&input.provider_id, &sealed, &fields)?;
    Ok(())
}
#[tauri::command]
fn remove_provider_secret(
    provider_id: String,
    field_id: String,
    state: State<Mutex<AppCoreState>>,
) -> Result<(), AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    let definition = providers::registry::definitions()
        .into_iter()
        .find(|value| value.id == provider_id)
        .ok_or(AppError::Stable("ERR_PROVIDER_NOT_FOUND"))?;
    let repo = state.repo()?;
    let raw = repo.provider_configuration(&provider_id, state.key()?)?;
    match remove_provider_secret_value(&definition, raw.as_ref(), &field_id)? {
        Some(value) => {
            let mut fields = value
                .as_object()
                .expect("provider configuration must be an object")
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            fields.sort();
            let sealed = crypto::seal(state.key()?, &serde_json::to_vec(&value)?)?;
            repo.upsert_provider_configuration(&provider_id, &sealed, &fields)?;
        }
        None => repo.remove_provider_configuration(&provider_id)?,
    }
    Ok(())
}
#[tauri::command]
fn remove_provider_credentials(
    provider_id: String,
    state: State<Mutex<AppCoreState>>,
) -> Result<(), AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    state.repo()?.remove_provider_configuration(&provider_id)?;
    Ok(())
}

#[tauri::command]
fn load_settings(state: State<Mutex<AppCoreState>>) -> Result<serde_json::Value, AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    Ok(state.repo()?.settings()?)
}
#[tauri::command]
fn save_setting(
    key: String,
    value: serde_json::Value,
    state: State<Mutex<AppCoreState>>,
) -> Result<(), AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    state.repo()?.save_setting(&key, &value)?;
    Ok(())
}

#[tauri::command]
async fn select_files(
    purpose: Option<String>,
    state: State<'_, Mutex<AppCoreState>>,
) -> Result<Vec<SelectedFile>, AppError> {
    begin_operation(&state)?;
    let result = async {
        let recording_only = match purpose.as_deref() {
            None | Some("meeting_material") => false,
            Some("recording") => true,
            _ => return Err(AppError::Stable("ERR_JOB_PAYLOAD")),
        };
        let selections = media::select_files(recording_only)
            .await
            .map_err(AppError::Stable)?;
        require_single_upload_file_count(selections.len())?;
        let metadata = selections.iter().map(|v| v.metadata.clone()).collect();
        let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        state.key()?;
        state.selections.clear();
        for selection in selections {
            state
                .selections
                .insert(selection.metadata.selection_token.clone(), selection);
        }
        Ok(metadata)
    }
    .await;
    finish_operation(&state);
    result
}
#[tauri::command]
fn audio_devices(
    state: State<Mutex<AppCoreState>>,
) -> Result<Vec<audio::AudioDeviceInfo>, AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    audio::input_devices().map_err(AppError::Stable)
}
#[tauri::command]
async fn sound_check(
    device_id: String,
    state: State<'_, Mutex<AppCoreState>>,
) -> Result<audio::SoundCheck, AppError> {
    begin_operation(&state)?;
    let result = audio::sound_check(&device_id)
        .await
        .map_err(AppError::Stable);
    finish_operation(&state);
    result
}
#[tauri::command]
fn system_audio_status(
    state: State<Mutex<AppCoreState>>,
) -> Result<platform::SystemAudioStatus, AppError> {
    {
        let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        state.key()?;
    }
    Ok(platform::adapter().status())
}
#[tauri::command]
fn request_system_audio_permission(
    state: State<Mutex<AppCoreState>>,
) -> Result<platform::SystemAudioStatus, AppError> {
    {
        let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        state.key()?;
    }
    Ok(platform::adapter().request_permission())
}
#[tauri::command]
fn open_system_audio_settings(state: State<Mutex<AppCoreState>>) -> Result<(), AppError> {
    {
        let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        state.key()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn()
            .map_err(|_| AppError::Stable("ERR_SYSTEM_SETTINGS"))?;
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    Err(AppError::Stable("ERR_SYSTEM_AUDIO_UNSUPPORTED"))
}

#[tauri::command]
fn list_projects(state: State<Mutex<AppCoreState>>) -> Result<Vec<MeetingProject>, AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    Ok(state.repo()?.list_projects()?)
}
#[tauri::command]
fn get_project_detail(
    project_id: String,
    state: State<Mutex<AppCoreState>>,
) -> Result<ProjectDetail, AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    Ok(state.repo()?.project_detail(&project_id, state.key()?)?)
}

#[tauri::command]
fn create_realtime_project(
    input: RealtimeStartInput,
    app: AppHandle,
    state: State<Mutex<AppCoreState>>,
) -> Result<ProjectDetail, AppError> {
    begin_operation(&state)?;
    let result = (|| -> Result<ProjectDetail, AppError> {
        let (repo, master, provider, project, session);
        {
            let mut state_guard = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
            state_guard
                .realtime_sessions
                .retain(|_, runtime| !runtime.is_completed());
            if state_guard
                .realtime_sessions
                .values()
                .any(|runtime| runtime.is_active())
            {
                return Err(AppError::Stable("ERR_ACTIVE_SESSION"));
            }
            if state_guard
                .realtime_sessions
                .values()
                .any(|runtime| runtime.cleanup_pending())
            {
                return Err(AppError::Stable("ERR_REALTIME_CLEANUP_BUSY"));
            }
            repo = state_guard.repo()?;
            master = state_guard.cloned_key()?;
            provider = providers::registry::resolve(&input.provider_id, &repo, &master)
                .map_err(AppError::Stable)?;
            let capabilities = provider.capabilities();
            let mut required = vec![
                "realtime_transcription",
                "segment_understanding",
                "meeting_synthesis",
                "communication_review",
                "meeting_minutes",
            ];
            if input.translation_target_language.is_some() {
                required.push("text_translation");
            }
            providers::registry::require(&capabilities, &required).map_err(AppError::Stable)?;
            providers::registry::validate_language_contract(
                &capabilities,
                input.source_language.as_deref(),
                input.translation_target_language.as_deref(),
                &[input.analysis_output_language.as_str()],
            )
            .map_err(AppError::Stable)?;
            let title = input
                .title
                .as_deref()
                .filter(|v| !v.trim().is_empty())
                .ok_or(AppError::Stable("ERR_TITLE_REQUIRED"))?;
            project =
                repo.create_project(title, input.mode.into(), ProjectStatus::Active, &master)?;
            session = repo.create_realtime_session(&project.id, input.mode)?;
        }
        let mut runtime = match realtime::start(
            app.clone(),
            repo.clone(),
            master.clone(),
            project.clone(),
            session.clone(),
            provider,
            realtime::StartOptions {
                device_id: input.device_id,
                mode: input.mode,
                provider_id: input.provider_id,
                source_language: input.source_language,
                translation_language: input.translation_target_language,
                analysis_language: input.analysis_output_language,
            },
        ) {
            Ok(runtime) => runtime,
            Err(code) => {
                repo.fail_realtime_session(&session.id, &project.id).ok();
                return Err(AppError::Stable(code));
            }
        };
        if let Err(error) = ensure_overlay(&app) {
            if !runtime.stop_and_wait(Duration::from_secs(10)) {
                runtime.mark_interrupted();
                if let Ok(mut state) = state.lock() {
                    state.realtime_sessions.insert(project.id.clone(), runtime);
                }
            }
            repo.fail_realtime_session(&session.id, &project.id).ok();
            realtime::complete_realtime_spool(&repo, &project.id).ok();
            return Err(error);
        }
        app.emit(
            "accordmesh://realtime-state",
            serde_json::json!({"projectId":project.id.clone(),"status":"running"}),
        )
        .ok();
        {
            let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
            state.realtime_sessions.insert(project.id.clone(), runtime);
        }
        Ok(repo.project_detail(&project.id, &master)?)
    })();
    finish_operation(&state);
    result
}

fn require_active_realtime<'a>(
    state: &'a AppCoreState,
    project_id: &str,
) -> Result<&'a realtime::ActiveRealtime, AppError> {
    let runtime = state
        .realtime_sessions
        .get(project_id)
        .ok_or(AppError::Stable("ERR_SESSION_NOT_ACTIVE"))?;
    if !runtime.is_active() {
        return Err(AppError::Stable("ERR_SESSION_NOT_ACTIVE"));
    }
    Ok(runtime)
}

fn converge_realtime_stop_timeout(
    repo: &Repository,
    runtime: &mut realtime::ActiveRealtime,
    project_id: &str,
) -> Result<(), AppError> {
    runtime.mark_interrupted();
    repo.fail_realtime_session(&runtime.session_id, project_id)?;
    Ok(())
}

#[tauri::command]
fn pause_realtime(
    project_id: String,
    app: AppHandle,
    state: State<Mutex<AppCoreState>>,
) -> Result<(), AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    let runtime = require_active_realtime(&state, &project_id)?;
    runtime.pause();
    state
        .repo()?
        .set_realtime_status(&runtime.session_id, RealtimeSessionStatus::Paused)?;
    app.emit(
        "accordmesh://realtime-state",
        serde_json::json!({"projectId":project_id,"status":"paused"}),
    )
    .ok();
    Ok(())
}
#[tauri::command]
fn resume_realtime(
    project_id: String,
    app: AppHandle,
    state: State<Mutex<AppCoreState>>,
) -> Result<(), AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    let runtime = require_active_realtime(&state, &project_id)?;
    runtime.resume();
    state
        .repo()?
        .set_realtime_status(&runtime.session_id, RealtimeSessionStatus::Running)?;
    app.emit(
        "accordmesh://realtime-state",
        serde_json::json!({"projectId":project_id,"status":"running"}),
    )
    .ok();
    Ok(())
}
#[tauri::command]
fn analyze_now(project_id: String, state: State<Mutex<AppCoreState>>) -> Result<(), AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    require_active_realtime(&state, &project_id)?.analyze_now();
    Ok(())
}
#[tauri::command]
fn active_realtime_state(
    state: State<Mutex<AppCoreState>>,
) -> Result<Option<serde_json::Value>, AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    let mut active = state
        .realtime_sessions
        .iter()
        .filter(|(_, runtime)| runtime.is_active());
    let Some((project_id, runtime)) = active.next() else {
        return Ok(None);
    };
    if active.next().is_some() {
        return Err(AppError::Stable("ERR_ACTIVE_SESSION"));
    }
    let status = if runtime.is_paused() {
        "paused"
    } else {
        "running"
    };
    Ok(Some(
        serde_json::json!({"projectId":project_id,"status":status}),
    ))
}
#[tauri::command]
fn show_overlay(
    project_id: String,
    app: AppHandle,
    state: State<Mutex<AppCoreState>>,
) -> Result<(), AppError> {
    let status = {
        let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        state.key()?;
        let runtime = require_active_realtime(&state, &project_id)?;
        if runtime.is_paused() {
            "paused"
        } else {
            "running"
        }
    };
    ensure_overlay(&app)?;
    app.emit(
        "accordmesh://realtime-state",
        serde_json::json!({"projectId":project_id,"status":status}),
    )
    .ok();
    Ok(())
}
#[tauri::command]
fn stop_realtime(
    project_id: String,
    app: AppHandle,
    state: State<Mutex<AppCoreState>>,
) -> Result<ProjectDetail, AppError> {
    begin_operation(&state)?;
    let result = (|| -> Result<ProjectDetail, AppError> {
        let (repo, key, registry, mut runtime);
        {
            let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
            repo = state.repo()?;
            key = state.cloned_key()?;
            registry = state.job_runtimes.clone();
            runtime = state
                .realtime_sessions
                .remove(&project_id)
                .ok_or(AppError::Stable("ERR_SESSION_NOT_ACTIVE"))?;
        }
        if runtime.is_interrupted() {
            let cleanup_pending = runtime.cleanup_pending();
            let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
            if cleanup_pending {
                state.realtime_sessions.insert(project_id.clone(), runtime);
                return Err(AppError::Stable("ERR_REALTIME_CLEANUP_BUSY"));
            }
            return Ok(repo.project_detail(&project_id, &key)?);
        }
        if !runtime.stop_and_wait(Duration::from_secs(10)) {
            let convergence = converge_realtime_stop_timeout(&repo, &mut runtime, &project_id);
            {
                let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
                state.realtime_sessions.insert(project_id.clone(), runtime);
            }
            convergence?;
            app.emit(
                "accordmesh://realtime-state",
                serde_json::json!({"projectId":project_id.clone(),"status":"interrupted"}),
            )
            .ok();
            if let Some(window) = app.get_webview_window("overlay") {
                window.hide().ok();
            }
            return Err(AppError::Stable("ERR_REALTIME_STOP_TIMEOUT"));
        }
        if let Some(code) = runtime.terminal_error() {
            repo.fail_realtime_session(&runtime.session_id, &project_id)
                .ok();
            return Err(AppError::Stable(code));
        }
        repo.set_realtime_status(&runtime.session_id, RealtimeSessionStatus::Completed)?;
        repo.set_project_status(&project_id, ProjectStatus::Processing)?;
        app.emit(
            "accordmesh://realtime-state",
            serde_json::json!({"projectId":project_id.clone(),"status":"completed"}),
        )
        .ok();
        let payload = if let Some(session) = realtime::read_spool_session(&repo, &project_id)? {
            realtime::finalization_payload_from_session(&session)
        } else {
            serde_json::json!({
                "realtimeFinalize":true,
                "providerId":runtime.provider_id,
                "sourceLanguage":runtime.source_language,
                "translationLanguage":runtime.translation_language,
                "outputLanguage":runtime.output_language,
                "pendingRealtimeChunks":runtime.take_pending_chunks()
            })
        };
        let job_id = repo.queue_job(&project_id, None, "realtime_finalize", 30, &payload)?;
        jobs::spawn_realtime_finalization(app.clone(), repo.clone(), key.clone(), job_id, registry)
            .map_err(AppError::Stable)?;
        if let Some(window) = app.get_webview_window("overlay") {
            window.hide().ok();
        }
        Ok(repo.project_detail(&project_id, &key)?)
    })();
    finish_operation(&state);
    result
}

#[tauri::command]
async fn create_upload_project(
    input: UploadInput,
    app: AppHandle,
    state: State<'_, Mutex<AppCoreState>>,
) -> Result<ProjectDetail, AppError> {
    let (repo, master, selections) = take_selections(&state, &input.files)?;
    let result = async {
        let provider = providers::registry::resolve(&input.provider_id, &repo, &master)
            .map_err(AppError::Stable)?;
        check_upload_capabilities(
            provider.as_ref(),
            &selections,
            false,
            input.source_language.as_deref(),
            input.translation_target_language.as_deref(),
            &input.analysis_output_language,
            &input.minutes_output_language,
        )?;
        let queued = copy_selections(&repo, selections).await?;
        let title = input
            .title
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or(AppError::Stable("ERR_TITLE_REQUIRED"))?;
        let project = repo.create_project(
            title,
            ProjectOrigin::UploadOnly,
            ProjectStatus::Processing,
            &master,
        )?;
        queue_upload(
            app,
            &state,
            repo,
            master,
            project,
            queued,
            input.provider_id,
            input.source_language,
            input.translation_target_language,
            input.analysis_output_language,
            input.minutes_output_language,
            false,
        )
        .await
    }
    .await;
    finish_operation(&state);
    result
}

#[tauri::command]
async fn attach_upload(
    input: AttachInput,
    app: AppHandle,
    state: State<'_, Mutex<AppCoreState>>,
) -> Result<ProjectDetail, AppError> {
    {
        let mut state_guard = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        state_guard.key()?;
        state_guard
            .realtime_sessions
            .retain(|_, runtime| !runtime.is_completed());
        if let Some(runtime) = state_guard.realtime_sessions.get(&input.project_id) {
            if runtime.is_active() {
                return Err(AppError::Stable("ERR_ACTIVE_SESSION"));
            }
            if runtime.cleanup_pending() {
                return Err(AppError::Stable("ERR_REALTIME_CLEANUP_BUSY"));
            }
        }
        let project = state_guard
            .repo()?
            .list_projects()?
            .into_iter()
            .find(|value| value.id == input.project_id)
            .ok_or(AppError::Stable("ERR_PROJECT_NOT_FOUND"))?;
        ensure_attachment_project_eligible(&project)?;
    }
    let (repo, master, selections) = take_selections(&state, &input.files)?;
    let result = async {
        require_attachment_media(&selections)?;
        let project = repo
            .list_projects()?
            .into_iter()
            .find(|value| value.id == input.project_id)
            .ok_or(AppError::Stable("ERR_PROJECT_NOT_FOUND"))?;
        ensure_attachment_project_eligible(&project)?;
        let provider = providers::registry::resolve(&input.provider_id, &repo, &master)
            .map_err(AppError::Stable)?;
        check_upload_capabilities(
            provider.as_ref(),
            &selections,
            true,
            input.source_language.as_deref(),
            input.translation_target_language.as_deref(),
            &input.analysis_output_language,
            &input.minutes_output_language,
        )?;
        let queued = copy_selections(&repo, selections).await?;
        repo.set_project_status(&input.project_id, ProjectStatus::Processing)?;
        queue_upload(
            app,
            &state,
            repo,
            master,
            project,
            queued,
            input.provider_id,
            input.source_language,
            input.translation_target_language,
            input.analysis_output_language,
            input.minutes_output_language,
            true,
        )
        .await
    }
    .await;
    finish_operation(&state);
    result
}

#[tauri::command]
fn cancel_job(job_id: String, state: State<Mutex<AppCoreState>>) -> Result<(), AppError> {
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    let repo = state.repo()?;
    if jobs::runtime_exists(&state.job_runtimes, &job_id) {
        jobs::request_cancel(&state.job_runtimes, &job_id);
        repo.request_job_cancel(&job_id)?;
    } else {
        repo.cancel_job(&job_id)?;
    }
    Ok(())
}

#[tauri::command]
fn retry_job(
    job_id: String,
    app: AppHandle,
    state: State<Mutex<AppCoreState>>,
) -> Result<(), AppError> {
    begin_operation(&state)?;
    let result = (|| -> Result<(), AppError> {
        let (repo, key, registry);
        {
            let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
            if jobs::runtime_exists(&state.job_runtimes, &job_id) {
                return Err(AppError::Stable("ERR_JOB_ALREADY_RUNNING"));
            }
            repo = state.repo()?;
            key = state.cloned_key()?;
            registry = state.job_runtimes.clone();
            let status = repo
                .job_status(&job_id)?
                .ok_or(AppError::Stable("ERR_JOB_NOT_RETRYABLE"))?;
            if !matches!(status.as_str(), "failed" | "resumable" | "cancelled") {
                return Err(AppError::Stable("ERR_JOB_NOT_RETRYABLE"));
            }
            let (_, value) = repo.job_payload(&job_id)?;
            if let Ok(payload) = serde_json::from_value::<UploadJobPayload>(value) {
                for file in payload.files {
                    if repo.media_for_job(&job_id, &file.queued_file_id)?.is_none() {
                        return Err(AppError::Stable("ERR_MEDIA_SOURCE_MISSING"));
                    }
                }
            }
            if !repo.retry_job(&job_id)? {
                return Err(AppError::Stable("ERR_JOB_NOT_RETRYABLE"));
            }
        }
        jobs::spawn_recovered(app, repo, key, job_id, registry).map_err(AppError::Stable)
    })();
    finish_operation(&state);
    result
}

#[tauri::command]
fn rename_project(
    project_id: String,
    title: String,
    state: State<Mutex<AppCoreState>>,
) -> Result<MeetingProject, AppError> {
    if title.trim().is_empty() {
        return Err(AppError::Stable("ERR_TITLE_REQUIRED"));
    }
    let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    Ok(state.repo()?.rename_project(&project_id, title.trim())?)
}
#[tauri::command]
fn delete_project(project_id: String, state: State<Mutex<AppCoreState>>) -> Result<(), AppError> {
    let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    if let Some(runtime) = state.realtime_sessions.get(&project_id) {
        if runtime.is_active() {
            return Err(AppError::Stable("ERR_ACTIVE_SESSION"));
        }
        if runtime.cleanup_pending() {
            return Err(AppError::Stable("ERR_REALTIME_CLEANUP_BUSY"));
        }
    }
    let repository = state.repo()?;
    let project = repository
        .list_projects()?
        .into_iter()
        .find(|project| project.id == project_id)
        .ok_or(AppError::Stable("ERR_PROJECT_NOT_FOUND"))?;
    if let Some(code) = delete_guard_for_status(project.status) {
        return Err(AppError::Stable(code));
    }
    match repository.delete_project(&project_id) {
        Ok(()) => {
            state.realtime_sessions.remove(&project_id);
            Ok(())
        }
        Err(DeleteProjectError::ActiveJob) => Err(AppError::Stable("ERR_ACTIVE_JOB")),
        Err(DeleteProjectError::Sql(error)) => Err(AppError::Sql(error)),
    }
}
#[tauri::command]
async fn export_project(
    project_id: String,
    format: ExportFormat,
    selected_artifact_ids: Vec<String>,
    include_transcript: bool,
    state: State<'_, Mutex<AppCoreState>>,
) -> Result<String, AppError> {
    begin_operation(&state)?;
    let result = async {
        let detail = {
            let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
            state.repo()?.project_detail(&project_id, state.key()?)?
        };
        let detail =
            export::prepare_detail(detail, &selected_artifact_ids).map_err(AppError::Stable)?;
        export::choose_and_write(&detail, format, include_transcript)
            .await
            .map_err(AppError::Stable)
    }
    .await;
    finish_operation(&state);
    result
}

#[tauri::command]
fn regenerate_artifact(
    input: RegenerateInput,
    app: AppHandle,
    state: State<Mutex<AppCoreState>>,
) -> Result<(), AppError> {
    begin_operation(&state)?;
    let result = (|| -> Result<(), AppError> {
        let (repo, key, registry, job_id);
        {
            let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
            repo = state.repo()?;
            key = state.cloned_key()?;
            registry = state.job_runtimes.clone();
            let detail = repo.project_detail(&input.project_id, &key)?;
            validate_regeneration_request(&input, &detail).map_err(AppError::Stable)?;
            if repo
                .regeneration_job_for_request(&input.project_id, &input.request_id)?
                .is_some()
            {
                return Ok(());
            }
            let payload = json_for_regeneration(&input);
            job_id = repo.queue_job(&input.project_id, None, "regenerate", 40, &payload)?;
        }
        jobs::spawn_regeneration(app, repo, key, job_id, registry).map_err(AppError::Stable)
    })();
    finish_operation(&state);
    result
}

pub(crate) fn validate_regeneration_request(
    input: &RegenerateInput,
    detail: &ProjectDetail,
) -> Result<(), &'static str> {
    validate_regeneration_request_id(&input.request_id)?;
    if input.source_segment_ids.is_empty()
        || input
            .source_segment_ids
            .iter()
            .any(|id| !detail.timeline.iter().any(|segment| segment.id == *id))
    {
        return Err("ERR_JOB_PAYLOAD");
    }
    if input.artifact_type == "segment_understanding" && input.source_segment_ids.len() != 1 {
        return Err("ERR_JOB_PAYLOAD");
    }
    let unique_segments = input.source_segment_ids.iter().collect::<HashSet<_>>();
    if unique_segments.len() != input.source_segment_ids.len() {
        return Err("ERR_JOB_PAYLOAD");
    }
    if input
        .source_artifact_ids
        .iter()
        .any(|id| !detail.artifacts.iter().any(|artifact| artifact.id == *id))
    {
        return Err("ERR_JOB_PAYLOAD");
    }
    let unique_artifacts = input.source_artifact_ids.iter().collect::<HashSet<_>>();
    if unique_artifacts.len() != input.source_artifact_ids.len() {
        return Err("ERR_JOB_PAYLOAD");
    }
    if input.artifact_type == "meeting_minutes" {
        validate_minutes_source_artifacts(&input.source_artifact_ids, &detail.artifacts)?;
    }
    Ok(())
}

pub(crate) fn validate_regeneration_request_id(request_id: &str) -> Result<(), &'static str> {
    let value = request_id.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("ERR_JOB_PAYLOAD");
    }
    Ok(())
}

pub(crate) fn validate_minutes_source_artifacts(
    source_artifact_ids: &[String],
    artifacts: &[AnalysisArtifact],
) -> Result<(), &'static str> {
    if source_artifact_ids.len() != 2 {
        return Err("ERR_JOB_PAYLOAD");
    }
    let selected = source_artifact_ids
        .iter()
        .map(|id| {
            artifacts
                .iter()
                .find(|artifact| artifact.id == *id)
                .ok_or("ERR_JOB_PAYLOAD")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let analysis_count = selected
        .iter()
        .filter(|artifact| artifact.artifact_type == "post_meeting_analysis")
        .count();
    let review_count = selected
        .iter()
        .filter(|artifact| artifact.artifact_type == "communication_review")
        .count();
    if analysis_count != 1 || review_count != 1 {
        return Err("ERR_JOB_PAYLOAD");
    }
    Ok(())
}

fn consume_registered_selections(
    selections: &mut media::SelectionRegistry,
    files: &[UploadFileInput],
) -> Result<Vec<media::NativeSelection>, AppError> {
    let mut unique = HashSet::with_capacity(files.len());
    for file in files {
        if !unique.insert(file.selection_token.as_str())
            || !selections.contains_key(&file.selection_token)
        {
            return Err(AppError::Stable("ERR_FILE_SELECTION_EXPIRED"));
        }
    }
    let mut out = Vec::with_capacity(files.len());
    for file in files {
        out.push(
            selections
                .remove(&file.selection_token)
                .ok_or(AppError::Stable("ERR_FILE_SELECTION_EXPIRED"))?,
        );
    }
    Ok(out)
}
fn require_single_upload_file_count(count: usize) -> Result<(), AppError> {
    if count == 1 {
        Ok(())
    } else {
        Err(AppError::Stable("ERR_SINGLE_FILE_REQUIRED"))
    }
}
fn ensure_attachment_project_eligible(project: &MeetingProject) -> Result<(), AppError> {
    if !matches!(
        project.origin,
        ProjectOrigin::RealtimeOnline | ProjectOrigin::RealtimeInPerson
    ) {
        return Err(AppError::Stable("ERR_ATTACHMENT_REALTIME_ONLY"));
    }
    if !matches!(project.status, ProjectStatus::Completed) {
        return Err(AppError::Stable("ERR_ATTACHMENT_PROJECT_NOT_COMPLETED"));
    }
    if !project.media_asset_ids.is_empty() || project.has_comparison {
        return Err(AppError::Stable("ERR_ATTACHMENT_ALREADY_EXISTS"));
    }
    Ok(())
}
fn require_attachment_media(selections: &[media::NativeSelection]) -> Result<(), AppError> {
    if selections.len() == 1
        && matches!(
            selections[0].metadata.kind,
            MediaKind::Audio | MediaKind::Video
        )
    {
        Ok(())
    } else {
        Err(AppError::Stable("ERR_ATTACHMENT_MEDIA_REQUIRED"))
    }
}
fn begin_operation(state: &State<'_, Mutex<AppCoreState>>) -> Result<(), AppError> {
    let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    state.key()?;
    state.operations_in_flight += 1;
    Ok(())
}
fn take_selections(
    state: &State<'_, Mutex<AppCoreState>>,
    files: &[UploadFileInput],
) -> Result<(Repository, Zeroizing<Vec<u8>>, Vec<media::NativeSelection>), AppError> {
    require_single_upload_file_count(files.len())?;
    let mut state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
    let repo = state.repo()?;
    let key = state.cloned_key()?;
    let out = consume_registered_selections(&mut state.selections, files)?;
    state.operations_in_flight += 1;
    Ok((repo, key, out))
}
fn finish_operation(state: &State<'_, Mutex<AppCoreState>>) {
    if let Ok(mut state) = state.lock() {
        state.operations_in_flight = state.operations_in_flight.saturating_sub(1);
    }
}
async fn copy_selections(
    repo: &Repository,
    selections: Vec<media::NativeSelection>,
) -> Result<Vec<QueuedFile>, AppError> {
    let mut queued = Vec::new();
    for selection in selections {
        let copied = media::controlled_copy(&selection, &repo.data_dir().join("temp")).await;
        let (path, sha) = match copied {
            Ok(value) => value,
            Err(code) => {
                let temporary_paths = queued
                    .iter_mut()
                    .filter_map(|file: &mut QueuedFile| file.temp_path.take())
                    .collect::<Vec<_>>();
                media::remove_temporary(temporary_paths).await;
                return Err(AppError::Stable(code));
            }
        };
        queued.push(QueuedFile {
            queued_file_id: Uuid::new_v4().to_string(),
            temp_path: Some(path),
            asset_id: None,
            original_file_name: selection.metadata.original_file_name,
            kind: selection.metadata.kind,
            sha256: sha,
            size: selection.metadata.size,
            mime_type: selection.metadata.mime_type,
        });
    }
    Ok(queued)
}
fn check_upload_capabilities(
    provider: &dyn providers::Provider,
    selections: &[media::NativeSelection],
    comparison: bool,
    source_language: Option<&str>,
    translation_target_language: Option<&str>,
    analysis_output_language: &str,
    minutes_output_language: &str,
) -> Result<(), AppError> {
    let capabilities = provider.capabilities();
    let mut required = vec![
        "segment_understanding",
        "meeting_synthesis",
        "communication_review",
        "meeting_minutes",
    ];
    if translation_target_language.is_some() {
        required.push("text_translation");
    }
    if selections
        .iter()
        .any(|v| matches!(v.metadata.kind, MediaKind::Audio | MediaKind::Video))
    {
        required.push("file_transcription");
    }
    if comparison {
        required.push("comparison_report");
    }
    providers::registry::require(&capabilities, &required).map_err(AppError::Stable)?;
    providers::registry::validate_language_contract(
        &capabilities,
        source_language,
        translation_target_language,
        &[analysis_output_language, minutes_output_language],
    )
    .map_err(AppError::Stable)
}

pub(crate) fn delete_guard_for_status(status: ProjectStatus) -> Option<&'static str> {
    match status {
        ProjectStatus::Active => Some("ERR_ACTIVE_SESSION"),
        ProjectStatus::Processing => Some("ERR_ACTIVE_JOB"),
        ProjectStatus::Completed | ProjectStatus::Failed => None,
    }
}
async fn queue_upload(
    app: AppHandle,
    state: &State<'_, Mutex<AppCoreState>>,
    repo: Repository,
    master: Zeroizing<Vec<u8>>,
    project: MeetingProject,
    files: Vec<QueuedFile>,
    provider_id: String,
    source_language: Option<String>,
    translation: Option<String>,
    analysis: String,
    minutes: String,
    attach: bool,
) -> Result<ProjectDetail, AppError> {
    let mut payload = UploadJobPayload {
        files,
        provider_id,
        source_language,
        translation_target_language: translation,
        analysis_output_language: analysis,
        minutes_output_language: minutes,
        attach_to_existing: attach,
    };
    let job_id = repo.queue_job(
        &project.id,
        None,
        if attach {
            "attach_and_compare"
        } else {
            "post_meeting_pipeline"
        },
        20,
        &serde_json::to_value(&payload)?,
    )?;
    let project_key = repo.project_key(&project.id, &master)?;
    for index in 0..payload.files.len() {
        let file = payload.files[index].clone();
        let source = match file.temp_path.clone() {
            Some(path) => path,
            None => {
                return Err(fail_queued_upload(
                    &repo,
                    &job_id,
                    &project.id,
                    attach,
                    "ERR_MEDIA_SOURCE_MISSING",
                ))
            }
        };
        let guard = media::TemporaryPath::from_existing(source);
        let asset = match repo
            .import_media_asset(
                &project.id,
                &job_id,
                &file.queued_file_id,
                &file.original_file_name,
                file.kind,
                file.mime_type.clone(),
                guard.path(),
                &project_key,
            )
            .await
        {
            Ok(asset) => asset,
            Err(code) => {
                return Err(fail_queued_upload(
                    &repo,
                    &job_id,
                    &project.id,
                    attach,
                    code,
                ))
            }
        };
        if asset.sha256 != file.sha256 {
            repo.delete_media_asset(&asset.id)?;
            repo.update_job(
                &job_id,
                "failed",
                "importing",
                0.0,
                Some("ERR_MEDIA_CHANGED"),
            )?;
            repo.set_project_status(
                &project.id,
                if attach {
                    ProjectStatus::Completed
                } else {
                    ProjectStatus::Failed
                },
            )?;
            return Err(AppError::Stable("ERR_MEDIA_CHANGED"));
        }
        payload.files[index].asset_id = Some(asset.id);
        payload.files[index].temp_path = None;
        repo.update_job_payload(&job_id, &serde_json::to_value(&payload)?)?;
    }
    let registry = {
        let state = state.lock().map_err(|_| AppError::Stable("ERR_STATE"))?;
        state.job_runtimes.clone()
    };
    if let Err(code) =
        jobs::spawn_upload(app, repo.clone(), master.clone(), job_id.clone(), registry)
    {
        return Err(fail_queued_upload(
            &repo,
            &job_id,
            &project.id,
            attach,
            code,
        ));
    }
    Ok(repo.project_detail(&project.id, &master)?)
}
fn fail_queued_upload(
    repo: &Repository,
    job_id: &str,
    project_id: &str,
    attach: bool,
    code: &'static str,
) -> AppError {
    repo.update_job(job_id, "failed", "failed", 0.0, Some(code))
        .ok();
    repo.set_project_status(
        project_id,
        if attach {
            ProjectStatus::Completed
        } else {
            ProjectStatus::Failed
        },
    )
    .ok();
    AppError::Stable(code)
}
fn json_for_regeneration(input: &RegenerateInput) -> serde_json::Value {
    serde_json::json!({
        "requestId":input.request_id,
        "artifactType":input.artifact_type,
        "providerId":input.provider_id,
        "modelId":input.model_id,
        "outputLanguage":input.output_language,
        "sourceSegmentIds":input.source_segment_ids,
        "sourceArtifactIds":input.source_artifact_ids,
    })
}
fn ensure_overlay(app: &AppHandle) -> Result<(), AppError> {
    if let Some(window) = app.get_webview_window("overlay") {
        window.show().map_err(|_| AppError::Stable("ERR_OVERLAY"))?;
        window.set_always_on_top(true).ok();
        return Ok(());
    }
    WebviewWindowBuilder::new(
        app,
        "overlay",
        WebviewUrl::App("index.html?overlay=1".into()),
    )
    .title("AccordMesh")
    .inner_size(440.0, 620.0)
    .min_inner_size(320.0, 260.0)
    .always_on_top(true)
    .decorations(true)
    .build()
    .map_err(|_| AppError::Stable("ERR_OVERLAY"))?;
    Ok(())
}

pub fn run() {
    let builder = tauri::Builder::default()
        .manage(Mutex::new(AppCoreState::default()))
        .setup(|app| {
            let data_dir = app.path().app_data_dir().unwrap_or_else(|_| {
                dirs::data_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("AccordMesh")
            });
            reset::recover_interrupted_reset(&data_dir)?;
            std::fs::create_dir_all(&data_dir)?;
            let state = app.state::<Mutex<AppCoreState>>();
            let mut state = state
                .lock()
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "state"))?;
            state.data_dir = data_dir;
            state
                .repo()
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "repository"))?
                .initialize()
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "database"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            setup_status,
            create_vault,
            unlock,
            lock,
            reset_vault_status,
            reset_vault,
            provider_definitions,
            provider_configuration_status,
            save_provider_credentials,
            remove_provider_secret,
            remove_provider_credentials,
            load_settings,
            save_setting,
            select_files,
            audio_devices,
            sound_check,
            system_audio_status,
            request_system_audio_permission,
            open_system_audio_settings,
            list_projects,
            get_project_detail,
            create_realtime_project,
            pause_realtime,
            resume_realtime,
            analyze_now,
            active_realtime_state,
            show_overlay,
            stop_realtime,
            create_upload_project,
            attach_upload,
            cancel_job,
            retry_job,
            rename_project,
            delete_project,
            export_project,
            regenerate_artifact
        ]);
    builder
        .run(tauri::generate_context!())
        .expect("failed to run AccordMesh");
}
