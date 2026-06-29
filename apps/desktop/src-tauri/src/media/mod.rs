use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto;
use crate::projects::types::{MediaKind, SelectedFile};
use crate::providers::TranscriptDraft;

pub const MAX_IMPORT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_TEXT_BYTES: u64 = 16 * 1024 * 1024;
const MEDIA_MAGIC: &[u8; 8] = b"AMMEDIA1";
const MEDIA_VERSION: u16 = 1;
const MEDIA_CHUNK_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedMediaManifest {
    pub format_version: u16,
    pub original_size: u64,
    pub sha256: String,
    pub chunk_count: u64,
    pub chunk_size: u32,
    pub original_file_name: String,
    pub mime_type: Option<String>,
    pub kind: MediaKind,
}

pub struct TemporaryPath {
    path: PathBuf,
}

pub struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    pub fn create(root: &Path, label: &str) -> Result<Self, &'static str> {
        let path = root.join(format!("{label}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).map_err(|_| "ERR_IO")?;
        Ok(Self { path })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl TemporaryPath {
    pub fn from_existing(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Clone)]
pub struct NativeSelection {
    pub path: PathBuf,
    pub metadata: SelectedFile,
}
pub type SelectionRegistry = HashMap<String, NativeSelection>;

pub async fn select_files(recording_only: bool) -> Result<Vec<NativeSelection>, &'static str> {
    let extensions: &[&str] = if recording_only {
        &["mp3", "wav", "m4a", "mp4", "mov", "webm", "mpeg", "mpga"]
    } else {
        &[
            "mp3", "wav", "m4a", "mp4", "mov", "webm", "mpeg", "mpga", "txt", "srt", "vtt",
        ]
    };
    let label = if recording_only {
        "Meeting recording"
    } else {
        "Meeting material"
    };
    let handle = rfd::AsyncFileDialog::new()
        .add_filter(label, extensions)
        .pick_file()
        .await
        .ok_or("ERR_FILE_SELECTION_CANCELLED")?;
    let canonical = std::fs::canonicalize(handle.path()).map_err(|_| "ERR_MEDIA_READ")?;
    let metadata = std::fs::metadata(&canonical).map_err(|_| "ERR_MEDIA_READ")?;
    if !metadata.is_file() {
        return Err("ERR_MEDIA_INVALID");
    }
    if metadata.len() == 0 || metadata.len() > MAX_IMPORT_BYTES {
        return Err("ERR_MEDIA_SIZE");
    }
    let name = canonical
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("ERR_MEDIA_INVALID")?
        .to_string();
    let kind = detect_kind(&canonical)?;
    let token = Uuid::new_v4().to_string();
    Ok(vec![NativeSelection {
        path: canonical,
        metadata: SelectedFile {
            selection_token: token,
            original_file_name: name,
            kind,
            size: metadata.len(),
            mime_type: mime_guess::from_path(handle.path())
                .first_raw()
                .map(str::to_string),
        },
    }])
}

pub async fn controlled_copy(
    selection: &NativeSelection,
    temp_root: &Path,
) -> Result<(PathBuf, String), &'static str> {
    let current = std::fs::canonicalize(&selection.path).map_err(|_| "ERR_MEDIA_READ")?;
    if current != selection.path {
        return Err("ERR_MEDIA_CHANGED");
    }
    tokio::fs::create_dir_all(temp_root)
        .await
        .map_err(|_| "ERR_IO")?;
    let destination = temp_root.join(format!("{}.import", selection.metadata.selection_token));
    let copy_result = async {
        let mut source = tokio::fs::File::open(&current)
            .await
            .map_err(|_| "ERR_MEDIA_READ")?;
        let mut target = tokio::fs::File::create(&destination)
            .await
            .map_err(|_| "ERR_IO")?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            let read = source
                .read(&mut buffer)
                .await
                .map_err(|_| "ERR_MEDIA_READ")?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            target
                .write_all(&buffer[..read])
                .await
                .map_err(|_| "ERR_IO")?;
        }
        target.flush().await.map_err(|_| "ERR_IO")?;
        Ok::<String, &'static str>(format!("{:x}", hasher.finalize()))
    }
    .await;
    match copy_result {
        Ok(sha) => Ok((destination, sha)),
        Err(code) => {
            tokio::fs::remove_file(&destination).await.ok();
            Err(code)
        }
    }
}

pub async fn sha256_file(path: &Path) -> Result<String, &'static str> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| "ERR_MEDIA_READ")?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let read = file.read(&mut buffer).await.map_err(|_| "ERR_MEDIA_READ")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub async fn encrypt_managed_file(
    source: &Path,
    destination: &Path,
    key: &[u8],
    original_file_name: &str,
    mime_type: Option<String>,
    kind: MediaKind,
) -> Result<ManagedMediaManifest, &'static str> {
    let mut input = tokio::fs::File::open(source)
        .await
        .map_err(|_| "ERR_MEDIA_READ")?;
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| "ERR_IO")?;
    }
    let temporary = destination.with_extension("media.tmp");
    let mut output = tokio::fs::File::create(&temporary)
        .await
        .map_err(|_| "ERR_IO")?;
    output.write_all(MEDIA_MAGIC).await.map_err(|_| "ERR_IO")?;
    output
        .write_all(&MEDIA_VERSION.to_le_bytes())
        .await
        .map_err(|_| "ERR_IO")?;
    output
        .write_all(&(MEDIA_CHUNK_SIZE as u32).to_le_bytes())
        .await
        .map_err(|_| "ERR_IO")?;
    output
        .write_all(&0u64.to_le_bytes())
        .await
        .map_err(|_| "ERR_IO")?;
    output
        .write_all(&0u64.to_le_bytes())
        .await
        .map_err(|_| "ERR_IO")?;

    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut chunks = 0u64;
    let mut buffer = Zeroizing::new(vec![0u8; MEDIA_CHUNK_SIZE]);
    loop {
        let read = input
            .read(&mut buffer)
            .await
            .map_err(|_| "ERR_MEDIA_READ")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
        let envelope = crypto::seal(key, &buffer[..read]).map_err(|_| "ERR_CRYPTO")?;
        let encoded = crypto::to_vec(&envelope).map_err(|_| "ERR_CRYPTO")?;
        output
            .write_all(&(encoded.len() as u32).to_le_bytes())
            .await
            .map_err(|_| "ERR_IO")?;
        output.write_all(&encoded).await.map_err(|_| "ERR_IO")?;
        chunks += 1;
    }

    let manifest = ManagedMediaManifest {
        format_version: MEDIA_VERSION,
        original_size: total,
        sha256: format!("{:x}", hasher.finalize()),
        chunk_count: chunks,
        chunk_size: MEDIA_CHUNK_SIZE as u32,
        original_file_name: original_file_name.into(),
        mime_type,
        kind,
    };
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|_| "ERR_JSON")?;
    let encoded_manifest =
        crypto::to_vec(&crypto::seal(key, &manifest_bytes).map_err(|_| "ERR_CRYPTO")?)
            .map_err(|_| "ERR_CRYPTO")?;
    let manifest_offset = output.stream_position().await.map_err(|_| "ERR_IO")?;
    output
        .write_all(&(encoded_manifest.len() as u32).to_le_bytes())
        .await
        .map_err(|_| "ERR_IO")?;
    output
        .write_all(&encoded_manifest)
        .await
        .map_err(|_| "ERR_IO")?;
    output
        .seek(std::io::SeekFrom::Start(14))
        .await
        .map_err(|_| "ERR_IO")?;
    output
        .write_all(&chunks.to_le_bytes())
        .await
        .map_err(|_| "ERR_IO")?;
    output
        .write_all(&manifest_offset.to_le_bytes())
        .await
        .map_err(|_| "ERR_IO")?;
    output.flush().await.map_err(|_| "ERR_IO")?;
    drop(output);
    tokio::fs::rename(&temporary, destination)
        .await
        .map_err(|_| "ERR_IO")?;
    Ok(manifest)
}

pub async fn decrypt_managed_file(
    source: &Path,
    destination: &Path,
    key: &[u8],
) -> Result<ManagedMediaManifest, &'static str> {
    let mut input = tokio::fs::File::open(source)
        .await
        .map_err(|_| "ERR_MEDIA_READ")?;
    let mut magic = [0u8; 8];
    input
        .read_exact(&mut magic)
        .await
        .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
    if &magic != MEDIA_MAGIC {
        return Err("ERR_ENCRYPTED_DATA_VERSION");
    }
    if read_u16(&mut input).await? != MEDIA_VERSION {
        return Err("ERR_ENCRYPTED_DATA_VERSION");
    }
    let _chunk_size = read_u32(&mut input).await?;
    let chunks = read_u64(&mut input).await?;
    let manifest_offset = read_u64(&mut input).await?;
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| "ERR_IO")?;
    }
    let mut output = tokio::fs::File::create(destination)
        .await
        .map_err(|_| "ERR_IO")?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    for _ in 0..chunks {
        let length = read_u32(&mut input).await? as usize;
        if length == 0 || length > MEDIA_CHUNK_SIZE * 2 {
            return Err("ERR_ENCRYPTED_DATA_CORRUPT");
        }
        let mut encoded = vec![0u8; length];
        input
            .read_exact(&mut encoded)
            .await
            .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
        let envelope = crypto::from_slice(&encoded).map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
        let plaintext = crypto::open(key, &envelope).map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
        hasher.update(&plaintext);
        total += plaintext.len() as u64;
        output.write_all(&plaintext).await.map_err(|_| "ERR_IO")?;
    }
    if input.stream_position().await.map_err(|_| "ERR_IO")? != manifest_offset {
        return Err("ERR_ENCRYPTED_DATA_CORRUPT");
    }
    let length = read_u32(&mut input).await? as usize;
    if length == 0 || length > 1024 * 1024 {
        return Err("ERR_ENCRYPTED_DATA_CORRUPT");
    }
    let mut encoded = vec![0u8; length];
    input
        .read_exact(&mut encoded)
        .await
        .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
    let envelope = crypto::from_slice(&encoded).map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
    let manifest: ManagedMediaManifest = serde_json::from_slice(
        &crypto::open(key, &envelope).map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?,
    )
    .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
    output.flush().await.map_err(|_| "ERR_IO")?;
    if manifest.original_size != total
        || manifest.chunk_count != chunks
        || manifest.sha256 != format!("{:x}", hasher.finalize())
    {
        return Err("ERR_ENCRYPTED_DATA_CORRUPT");
    }
    Ok(manifest)
}

async fn read_u16(file: &mut tokio::fs::File) -> Result<u16, &'static str> {
    let mut bytes = [0u8; 2];
    file.read_exact(&mut bytes)
        .await
        .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
    Ok(u16::from_le_bytes(bytes))
}

async fn read_u32(file: &mut tokio::fs::File) -> Result<u32, &'static str> {
    let mut bytes = [0u8; 4];
    file.read_exact(&mut bytes)
        .await
        .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
    Ok(u32::from_le_bytes(bytes))
}

async fn read_u64(file: &mut tokio::fs::File) -> Result<u64, &'static str> {
    let mut bytes = [0u8; 8];
    file.read_exact(&mut bytes)
        .await
        .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?;
    Ok(u64::from_le_bytes(bytes))
}

pub fn parse_text(kind: MediaKind, bytes: &[u8]) -> Result<Vec<TranscriptDraft>, &'static str> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "ERR_TEXT_ENCODING")?
        .replace("\r\n", "\n");
    match kind {
        MediaKind::Transcript => Ok(text
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let line = line.trim();
                if line.is_empty() {
                    None
                } else {
                    Some(TranscriptDraft {
                        start_ms: index as i64 * 5000,
                        end_ms: (index as i64 + 1) * 5000,
                        text: line.into(),
                        detected_language: None,
                        confidence: None,
                    })
                }
            })
            .collect()),
        MediaKind::Subtitle => parse_subtitles(&text),
        _ => Err("ERR_MEDIA_INVALID"),
    }
}

fn parse_subtitles(text: &str) -> Result<Vec<TranscriptDraft>, &'static str> {
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let mut lines = block.lines().filter(|line| !line.trim().is_empty());
        let first = lines.next().unwrap_or("");
        let timing = if first.contains("-->") {
            first
        } else {
            lines.next().unwrap_or("")
        };
        let Some((start, end)) = timing.split_once("-->") else {
            continue;
        };
        let body = lines
            .filter(|line| !line.starts_with("NOTE") && !line.starts_with("WEBVTT"))
            .collect::<Vec<_>>()
            .join(" ");
        if !body.trim().is_empty() {
            out.push(TranscriptDraft {
                start_ms: timestamp_ms(start.trim())?,
                end_ms: timestamp_ms(end.split_whitespace().next().unwrap_or(""))?,
                text: body,
                detected_language: None,
                confidence: None,
            });
        }
    }
    if out.is_empty() {
        Err("ERR_SUBTITLE_PARSE")
    } else {
        out.sort_by_key(|v| v.start_ms);
        Ok(out)
    }
}

fn timestamp_ms(value: &str) -> Result<i64, &'static str> {
    let normalized = value.replace(',', ".");
    let parts = normalized.split(':').collect::<Vec<_>>();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [m, s] => (
            0,
            m.parse::<i64>().map_err(|_| "ERR_SUBTITLE_PARSE")?,
            s.parse::<f64>().map_err(|_| "ERR_SUBTITLE_PARSE")?,
        ),
        [h, m, s] => (
            h.parse::<i64>().map_err(|_| "ERR_SUBTITLE_PARSE")?,
            m.parse::<i64>().map_err(|_| "ERR_SUBTITLE_PARSE")?,
            s.parse::<f64>().map_err(|_| "ERR_SUBTITLE_PARSE")?,
        ),
        _ => return Err("ERR_SUBTITLE_PARSE"),
    };
    Ok(hours * 3_600_000 + minutes * 60_000 + (seconds * 1000.0) as i64)
}

pub const TRANSCRIPTION_UPLOAD_LIMIT_BYTES: u64 = 25 * 1024 * 1024;
pub const TRANSCRIPTION_UPLOAD_SAFE_BYTES: u64 = 24 * 1024 * 1024;
const TRANSCRIPTION_CHUNK_MS: i64 = 10 * 60 * 1000;
const TRANSCRIPTION_CHUNK_OVERLAP_MS: i64 = 1000;
const MEDIA_PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);
const DEFAULT_MEDIA_PROCESS_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const DEFAULT_MEDIA_PROCESS_OUTPUT_LIMIT: usize = 64 * 1024;

#[derive(Clone)]
pub(crate) struct MediaProcessPolicy {
    pub ffmpeg_path: PathBuf,
    pub ffprobe_path: PathBuf,
    pub timeout: Duration,
    pub output_limit: usize,
}

impl Default for MediaProcessPolicy {
    fn default() -> Self {
        Self {
            ffmpeg_path: media_tool_path("ACCORDMESH_TEST_FFMPEG_BIN", "ffmpeg"),
            ffprobe_path: media_tool_path("ACCORDMESH_TEST_FFPROBE_BIN", "ffprobe"),
            timeout: DEFAULT_MEDIA_PROCESS_TIMEOUT,
            output_limit: DEFAULT_MEDIA_PROCESS_OUTPUT_LIMIT,
        }
    }
}

fn media_tool_path(test_variable: &str, fallback: &str) -> PathBuf {
    #[cfg(test)]
    if let Some(value) = std::env::var_os(test_variable) {
        return PathBuf::from(value);
    }
    let _ = test_variable;
    PathBuf::from(fallback)
}

#[derive(Clone)]
pub struct PreparedChunk {
    pub path: PathBuf,
    pub start_ms: i64,
    pub end_ms: i64,
    pub overlap_ms: i64,
}

struct ProcessCapture {
    stdout: Vec<u8>,
}

#[cfg(test)]
pub async fn prepare_media(
    path: &Path,
    kind: MediaKind,
    temp_root: &Path,
) -> Result<(Vec<PreparedChunk>, Option<i64>), &'static str> {
    prepare_media_cancellable(path, kind, temp_root, &Arc::new(AtomicBool::new(false))).await
}

pub async fn prepare_media_cancellable(
    path: &Path,
    kind: MediaKind,
    temp_root: &Path,
    cancelled: &Arc<AtomicBool>,
) -> Result<(Vec<PreparedChunk>, Option<i64>), &'static str> {
    prepare_media_with_policy(
        path,
        kind,
        temp_root,
        cancelled,
        &MediaProcessPolicy::default(),
    )
    .await
}

pub(crate) async fn prepare_media_with_policy(
    path: &Path,
    kind: MediaKind,
    temp_root: &Path,
    cancelled: &Arc<AtomicBool>,
    policy: &MediaProcessPolicy,
) -> Result<(Vec<PreparedChunk>, Option<i64>), &'static str> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("ERR_JOB_CANCELLED");
    }
    tokio::fs::create_dir_all(temp_root)
        .await
        .map_err(|_| "ERR_IO")?;
    let source = if matches!(kind, MediaKind::Video) {
        ensure_audio_track(path, cancelled, policy).await?;
        let extracted = temp_root.join(format!("{}.wav", Uuid::new_v4()));
        let args = vec![
            OsString::from("-nostdin"),
            OsString::from("-hide_banner"),
            OsString::from("-loglevel"),
            OsString::from("error"),
            OsString::from("-y"),
            OsString::from("-i"),
            path.as_os_str().to_os_string(),
            OsString::from("-map"),
            OsString::from("0:a:0"),
            OsString::from("-vn"),
            OsString::from("-ac"),
            OsString::from("1"),
            OsString::from("-ar"),
            OsString::from("16000"),
            OsString::from("-c:a"),
            OsString::from("pcm_s16le"),
            extracted.as_os_str().to_os_string(),
        ];
        run_tool(
            &policy.ffmpeg_path,
            &args,
            cancelled,
            policy,
            "ERR_MEDIA_FFMPEG_UNAVAILABLE",
            "ERR_MEDIA_EXTRACTION",
        )
        .await?;
        require_nonempty_output(&extracted).await?;
        extracted
    } else {
        path.to_path_buf()
    };
    let source_size = tokio::fs::metadata(&source)
        .await
        .map_err(|_| "ERR_MEDIA_READ")?
        .len();
    if source_size == 0 {
        return Err("ERR_MEDIA_OUTPUT_EMPTY");
    }
    let duration = probe_duration(&source, cancelled, policy).await?;
    if duration <= 0 {
        return Err("ERR_MEDIA_DURATION");
    }
    let duration_requires_chunking = duration > 20 * 60 * 1000;
    let size_requires_chunking = source_size >= TRANSCRIPTION_UPLOAD_SAFE_BYTES;
    if !duration_requires_chunking && !size_requires_chunking {
        return Ok((
            vec![PreparedChunk {
                path: source,
                start_ms: 0,
                end_ms: duration,
                overlap_ms: 0,
            }],
            Some(duration),
        ));
    }
    let mut chunks = Vec::new();
    let mut start = 0i64;
    let mut index = 0;
    while start < duration {
        if cancelled.load(Ordering::Relaxed) {
            return Err("ERR_JOB_CANCELLED");
        }
        let end = (start + TRANSCRIPTION_CHUNK_MS).min(duration);
        let output = temp_root.join(format!("chunk-{index}.wav"));
        let args = vec![
            OsString::from("-nostdin"),
            OsString::from("-hide_banner"),
            OsString::from("-loglevel"),
            OsString::from("error"),
            OsString::from("-y"),
            OsString::from("-ss"),
            OsString::from(format!("{:.3}", start as f64 / 1000.0)),
            OsString::from("-i"),
            source.as_os_str().to_os_string(),
            OsString::from("-t"),
            OsString::from(format!("{:.3}", (end - start) as f64 / 1000.0)),
            OsString::from("-map"),
            OsString::from("0:a:0"),
            OsString::from("-vn"),
            OsString::from("-ac"),
            OsString::from("1"),
            OsString::from("-ar"),
            OsString::from("16000"),
            OsString::from("-c:a"),
            OsString::from("pcm_s16le"),
            output.as_os_str().to_os_string(),
        ];
        run_tool(
            &policy.ffmpeg_path,
            &args,
            cancelled,
            policy,
            "ERR_MEDIA_FFMPEG_UNAVAILABLE",
            "ERR_MEDIA_EXTRACTION",
        )
        .await?;
        let output_size = require_nonempty_output(&output).await?;
        if output_size >= TRANSCRIPTION_UPLOAD_LIMIT_BYTES {
            return Err("ERR_MEDIA_OUTPUT_TOO_LARGE");
        }
        chunks.push(PreparedChunk {
            path: output,
            start_ms: start,
            end_ms: end,
            overlap_ms: if index == 0 {
                0
            } else {
                TRANSCRIPTION_CHUNK_OVERLAP_MS
            },
        });
        if end == duration {
            break;
        }
        start = end - TRANSCRIPTION_CHUNK_OVERLAP_MS;
        index += 1;
    }
    Ok((chunks, Some(duration)))
}

#[cfg(test)]
pub(crate) async fn media_tool_versions_with_policy(
    cancelled: &Arc<AtomicBool>,
    policy: &MediaProcessPolicy,
) -> Result<(String, String), &'static str> {
    let ffmpeg = run_tool(
        &policy.ffmpeg_path,
        &[OsString::from("-version")],
        cancelled,
        policy,
        "ERR_MEDIA_FFMPEG_UNAVAILABLE",
        "ERR_MEDIA_TOOL_VERSION",
    )
    .await?;
    let ffprobe = run_tool(
        &policy.ffprobe_path,
        &[OsString::from("-version")],
        cancelled,
        policy,
        "ERR_MEDIA_FFPROBE_UNAVAILABLE",
        "ERR_MEDIA_TOOL_VERSION",
    )
    .await?;
    let ffmpeg = first_output_line(ffmpeg.stdout, "ffmpeg version")?;
    let ffprobe = first_output_line(ffprobe.stdout, "ffprobe version")?;
    Ok((ffmpeg, ffprobe))
}

#[cfg(test)]
fn first_output_line(bytes: Vec<u8>, prefix: &str) -> Result<String, &'static str> {
    let text = String::from_utf8(bytes).map_err(|_| "ERR_MEDIA_TOOL_VERSION")?;
    let line = text.lines().next().unwrap_or("").trim();
    if !line.starts_with(prefix) {
        return Err("ERR_MEDIA_TOOL_VERSION");
    }
    Ok(line.to_string())
}

async fn ensure_audio_track(
    path: &Path,
    cancelled: &Arc<AtomicBool>,
    policy: &MediaProcessPolicy,
) -> Result<(), &'static str> {
    let args = vec![
        OsString::from("-v"),
        OsString::from("error"),
        OsString::from("-select_streams"),
        OsString::from("a:0"),
        OsString::from("-show_entries"),
        OsString::from("stream=index"),
        OsString::from("-of"),
        OsString::from("csv=p=0"),
        path.as_os_str().to_os_string(),
    ];
    let output = run_tool(
        &policy.ffprobe_path,
        &args,
        cancelled,
        policy,
        "ERR_MEDIA_FFPROBE_UNAVAILABLE",
        "ERR_MEDIA_CORRUPT",
    )
    .await?;
    if String::from_utf8(output.stdout)
        .map_err(|_| "ERR_MEDIA_CORRUPT")?
        .trim()
        .is_empty()
    {
        return Err("ERR_MEDIA_NO_AUDIO_TRACK");
    }
    Ok(())
}

async fn probe_duration(
    path: &Path,
    cancelled: &Arc<AtomicBool>,
    policy: &MediaProcessPolicy,
) -> Result<i64, &'static str> {
    let args = vec![
        OsString::from("-v"),
        OsString::from("error"),
        OsString::from("-show_entries"),
        OsString::from("format=duration"),
        OsString::from("-of"),
        OsString::from("default=noprint_wrappers=1:nokey=1"),
        path.as_os_str().to_os_string(),
    ];
    let output = run_tool(
        &policy.ffprobe_path,
        &args,
        cancelled,
        policy,
        "ERR_MEDIA_FFPROBE_UNAVAILABLE",
        "ERR_MEDIA_CORRUPT",
    )
    .await?;
    let seconds = String::from_utf8(output.stdout)
        .map_err(|_| "ERR_MEDIA_DURATION")?
        .trim()
        .parse::<f64>()
        .map_err(|_| "ERR_MEDIA_DURATION")?;
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("ERR_MEDIA_DURATION");
    }
    let milliseconds = (seconds * 1000.0).round();
    if milliseconds > i64::MAX as f64 {
        return Err("ERR_MEDIA_DURATION");
    }
    Ok(milliseconds as i64)
}

async fn require_nonempty_output(path: &Path) -> Result<u64, &'static str> {
    let size = tokio::fs::metadata(path)
        .await
        .map_err(|_| "ERR_MEDIA_OUTPUT_EMPTY")?
        .len();
    if size == 0 {
        return Err("ERR_MEDIA_OUTPUT_EMPTY");
    }
    Ok(size)
}

async fn run_tool(
    program: &Path,
    args: &[OsString],
    cancelled: &Arc<AtomicBool>,
    policy: &MediaProcessPolicy,
    unavailable_code: &'static str,
    failure_code: &'static str,
) -> Result<ProcessCapture, &'static str> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("ERR_JOB_CANCELLED");
    }
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| unavailable_code)?;
    let stdout = child.stdout.take().ok_or("ERR_MEDIA_TOOL_OUTPUT")?;
    let stderr = child.stderr.take().ok_or("ERR_MEDIA_TOOL_OUTPUT")?;
    let output_limit = policy.output_limit.max(1);
    let stdout_task = tokio::spawn(read_bounded(stdout, output_limit));
    let stderr_task = tokio::spawn(read_bounded(stderr, output_limit));
    let started = Instant::now();
    let status = loop {
        if cancelled.load(Ordering::Relaxed) {
            terminate_child(&mut child).await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err("ERR_JOB_CANCELLED");
        }
        if started.elapsed() >= policy.timeout {
            terminate_child(&mut child).await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err("ERR_MEDIA_TOOL_TIMEOUT");
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => tokio::time::sleep(MEDIA_PROCESS_POLL_INTERVAL).await,
            Err(_) => {
                terminate_child(&mut child).await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                return Err(failure_code);
            }
        }
    };
    let (stdout, stdout_truncated) = join_output(stdout_task).await?;
    let (_stderr, stderr_truncated) = join_output(stderr_task).await?;
    if stdout_truncated || stderr_truncated {
        return Err("ERR_MEDIA_TOOL_OUTPUT");
    }
    if !status.success() {
        return Err(failure_code);
    }
    Ok(ProcessCapture { stdout })
}

async fn terminate_child(child: &mut tokio::process::Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}

async fn join_output(
    handle: tokio::task::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
) -> Result<(Vec<u8>, bool), &'static str> {
    handle
        .await
        .map_err(|_| "ERR_MEDIA_TOOL_OUTPUT")?
        .map_err(|_| "ERR_MEDIA_TOOL_OUTPUT")
}

async fn read_bounded<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::with_capacity(limit.min(8192));
    let mut truncated = false;
    let mut buffer = [0u8; 8192];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(captured.len());
        let keep = remaining.min(read);
        if keep > 0 {
            captured.extend_from_slice(&buffer[..keep]);
        }
        if keep < read {
            truncated = true;
        }
    }
    Ok((captured, truncated))
}

pub fn detect_kind(path: &Path) -> Result<MediaKind, &'static str> {
    match path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "txt" => Ok(MediaKind::Transcript),
        "srt" | "vtt" => Ok(MediaKind::Subtitle),
        "mp3" | "wav" | "m4a" | "mpeg" | "mpga" => Ok(MediaKind::Audio),
        "mp4" | "mov" | "webm" => Ok(MediaKind::Video),
        _ => Err("ERR_MEDIA_FORMAT"),
    }
}

pub async fn remove_temporary(paths: impl IntoIterator<Item = PathBuf>) {
    for path in paths {
        tokio::fs::remove_file(path).await.ok();
    }
}
