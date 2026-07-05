use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

#[cfg(test)]
const LOCK_FILE: &str = include_str!("../../media-runtime.lock");
const RELEASE_FFMPEG_NAME: &str = "ffmpeg";
const RELEASE_FFPROBE_NAME: &str = "ffprobe";

#[derive(Debug, Clone)]
pub(crate) struct ResolvedMediaRuntime {
    pub ffmpeg_path: PathBuf,
    pub ffprobe_path: PathBuf,
    pub mode: &'static str,
    pub target: String,
    pub expected_version: String,
    pub ffmpeg_sha256: String,
    pub ffprobe_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRuntimeToolStatus {
    pub available: bool,
    pub integrity_verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub expected_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRuntimeStatus {
    pub available: bool,
    pub bundled: bool,
    pub mode: String,
    pub target: String,
    pub expected_version: String,
    pub ffmpeg: MediaRuntimeToolStatus,
    pub ffprobe: MediaRuntimeToolStatus,
}

#[derive(Debug, Clone)]
struct RuntimeLock {
    version: String,
    target: String,
    ffmpeg_sha256: String,
    ffprobe_sha256: String,
}

pub(crate) fn resolve() -> Result<ResolvedMediaRuntime, &'static str> {
    #[cfg(test)]
    {
        return resolve_test_runtime();
    }
    #[cfg(not(test))]
    {
        resolve_uncached()
    }
}

pub(crate) fn initial_status() -> (MediaRuntimeStatus, Option<ResolvedMediaRuntime>) {
    let lock = match parse_lock() {
        Ok(lock) => lock,
        Err(code) => return (unavailable_status(code, None), None),
    };
    match resolve() {
        Ok(runtime) => {
            let status = MediaRuntimeStatus {
                available: true,
                bundled: true,
                mode: runtime.mode.to_string(),
                target: runtime.target.clone(),
                expected_version: runtime.expected_version.clone(),
                ffmpeg: MediaRuntimeToolStatus {
                    available: true,
                    integrity_verified: true,
                    version: None,
                    sha256: Some(runtime.ffmpeg_sha256.clone()),
                    expected_sha256: lock.ffmpeg_sha256,
                    error_code: None,
                },
                ffprobe: MediaRuntimeToolStatus {
                    available: true,
                    integrity_verified: true,
                    version: None,
                    sha256: Some(runtime.ffprobe_sha256.clone()),
                    expected_sha256: lock.ffprobe_sha256,
                    error_code: None,
                },
            };
            (status, Some(runtime))
        }
        Err(code) => (unavailable_status(code, Some(lock)), None),
    }
}

fn unavailable_status(code: &'static str, lock: Option<RuntimeLock>) -> MediaRuntimeStatus {
    let lock = lock.unwrap_or_else(|| RuntimeLock {
        version: "unknown".into(),
        target: "unknown".into(),
        ffmpeg_sha256: "unknown".into(),
        ffprobe_sha256: "unknown".into(),
    });
    let tool = |expected_sha256: String| MediaRuntimeToolStatus {
        available: false,
        integrity_verified: false,
        version: None,
        sha256: None,
        expected_sha256,
        error_code: Some(code.to_string()),
    };
    MediaRuntimeStatus {
        available: false,
        bundled: true,
        mode: runtime_mode().to_string(),
        target: lock.target,
        expected_version: lock.version,
        ffmpeg: tool(lock.ffmpeg_sha256),
        ffprobe: tool(lock.ffprobe_sha256),
    }
}

fn resolve_uncached() -> Result<ResolvedMediaRuntime, &'static str> {
    let lock = parse_lock()?;
    if lock.target != env!("ACCORDMESH_TARGET_TRIPLE") {
        return Err("ERR_MEDIA_RUNTIME_TARGET");
    }
    let (ffmpeg_path, ffprobe_path, mode) = runtime_paths(&lock.target)?;
    let ffmpeg_sha256 = verify_binary(&ffmpeg_path, &lock.ffmpeg_sha256)?;
    let ffprobe_sha256 = verify_binary(&ffprobe_path, &lock.ffprobe_sha256)?;
    Ok(ResolvedMediaRuntime {
        ffmpeg_path,
        ffprobe_path,
        mode,
        target: lock.target,
        expected_version: lock.version,
        ffmpeg_sha256,
        ffprobe_sha256,
    })
}

#[cfg(test)]
fn resolve_test_runtime() -> Result<ResolvedMediaRuntime, &'static str> {
    let ffmpeg_path = std::env::var_os("ACCORDMESH_TEST_FFMPEG_BIN")
        .map(PathBuf::from)
        .ok_or("ERR_MEDIA_RUNTIME_MISSING")?;
    let ffprobe_path = std::env::var_os("ACCORDMESH_TEST_FFPROBE_BIN")
        .map(PathBuf::from)
        .ok_or("ERR_MEDIA_RUNTIME_MISSING")?;
    Ok(ResolvedMediaRuntime {
        ffmpeg_path,
        ffprobe_path,
        mode: "test_override",
        target: "test".into(),
        expected_version: "test".into(),
        ffmpeg_sha256: "test".into(),
        ffprobe_sha256: "test".into(),
    })
}

fn runtime_paths(target: &str) -> Result<(PathBuf, PathBuf, &'static str), &'static str> {
    #[cfg(debug_assertions)]
    {
        let base = if let Some(value) = std::env::var_os("ACCORDMESH_DEV_MEDIA_RUNTIME_DIR") {
            PathBuf::from(value)
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries")
        };
        return Ok((
            base.join(format!("{RELEASE_FFMPEG_NAME}-{target}")),
            base.join(format!("{RELEASE_FFPROBE_NAME}-{target}")),
            "development_bundled",
        ));
    }
    #[cfg(not(debug_assertions))]
    {
        let executable = std::env::current_exe().map_err(|_| "ERR_MEDIA_RUNTIME_LOCATION")?;
        let directory = release_macos_directory(&executable)?;
        Ok((
            directory.join(RELEASE_FFMPEG_NAME),
            directory.join(RELEASE_FFPROBE_NAME),
            "release_bundled",
        ))
    }
}

fn release_macos_directory(executable: &Path) -> Result<PathBuf, &'static str> {
    let directory = executable.parent().ok_or("ERR_MEDIA_RUNTIME_LOCATION")?;
    if directory.file_name().and_then(|value| value.to_str()) != Some("MacOS") {
        return Err("ERR_MEDIA_RUNTIME_LOCATION");
    }
    let contents = directory.parent().ok_or("ERR_MEDIA_RUNTIME_LOCATION")?;
    if contents.file_name().and_then(|value| value.to_str()) != Some("Contents") {
        return Err("ERR_MEDIA_RUNTIME_LOCATION");
    }
    let app = contents.parent().ok_or("ERR_MEDIA_RUNTIME_LOCATION")?;
    if app.extension().and_then(|value| value.to_str()) != Some("app") {
        return Err("ERR_MEDIA_RUNTIME_LOCATION");
    }
    Ok(directory.to_path_buf())
}

fn verify_binary(path: &Path, expected_sha256: &str) -> Result<String, &'static str> {
    let metadata = std::fs::metadata(path).map_err(|_| "ERR_MEDIA_RUNTIME_MISSING")?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("ERR_MEDIA_RUNTIME_MISSING");
    }
    let actual = sha256(path)?;
    if actual != expected_sha256 {
        return Err("ERR_MEDIA_RUNTIME_INTEGRITY");
    }
    Ok(actual)
}

fn sha256(path: &Path) -> Result<String, &'static str> {
    let mut file = File::open(path).map_err(|_| "ERR_MEDIA_RUNTIME_MISSING")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 256 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "ERR_MEDIA_RUNTIME_INTEGRITY")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn parse_lock() -> Result<RuntimeLock, &'static str> {
    let version = env!("ACCORDMESH_MEDIA_RUNTIME_VERSION");
    let target = env!("ACCORDMESH_MEDIA_RUNTIME_TARGET");
    let ffmpeg_sha256 = env!("ACCORDMESH_MEDIA_RUNTIME_FFMPEG_SHA256");
    let ffprobe_sha256 = env!("ACCORDMESH_MEDIA_RUNTIME_FFPROBE_SHA256");
    if version.is_empty()
        || target.is_empty()
        || ffmpeg_sha256.len() != 64
        || ffprobe_sha256.len() != 64
    {
        return Err("ERR_MEDIA_RUNTIME_LOCK");
    }
    Ok(RuntimeLock {
        version: version.into(),
        target: target.into(),
        ffmpeg_sha256: ffmpeg_sha256.into(),
        ffprobe_sha256: ffprobe_sha256.into(),
    })
}

fn runtime_mode() -> &'static str {
    #[cfg(test)]
    {
        return "test_override";
    }
    #[cfg(all(not(test), debug_assertions))]
    {
        return "development_bundled";
    }
    #[cfg(all(not(test), not(debug_assertions)))]
    {
        "release_bundled"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn lock_matches_frozen_r1b_candidate() {
        let source_values = LOCK_FILE
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect::<std::collections::HashMap<_, _>>();
        let lock = parse_lock().expect("parse media runtime lock");
        assert_eq!(source_values.get("format_version"), Some(&"1"));
        assert_eq!(lock.version, "8.1.2");
        assert_eq!(lock.target, "aarch64-apple-darwin");
        assert_eq!(
            source_values.get("ffmpeg_version"),
            Some(&lock.version.as_str())
        );
        assert_eq!(source_values.get("target"), Some(&lock.target.as_str()));
        assert_eq!(
            lock.ffmpeg_sha256,
            "30b1059d8c815fda3fa53a5c3b381bbe3f4aa45c937050a91cb14f6dbd333590"
        );
        assert_eq!(
            lock.ffprobe_sha256,
            "79aec0322c537e2b240fe4ce9db4e3bf29b06ee43cd72913ad0c4b7c505720fb"
        );
    }

    #[test]
    fn release_runtime_must_be_inside_app_contents_macos() {
        let valid = Path::new("/Applications/AccordMesh.app/Contents/MacOS/accordmesh");
        let expected = PathBuf::from("/Applications/AccordMesh.app/Contents/MacOS");
        assert_eq!(release_macos_directory(valid), Ok(expected));
        assert_eq!(
            release_macos_directory(Path::new("/usr/local/bin/accordmesh")),
            Err("ERR_MEDIA_RUNTIME_LOCATION"),
        );
    }

    #[test]
    fn integrity_check_fails_closed() {
        let path = std::env::temp_dir().join(format!(
            "accordmesh-runtime-integrity-{}",
            uuid::Uuid::new_v4()
        ));
        let mut file = File::create(&path).expect("create synthetic binary");
        file.write_all(b"not-the-approved-runtime")
            .expect("write synthetic binary");
        assert_eq!(
            verify_binary(
                &path,
                "0000000000000000000000000000000000000000000000000000000000000000"
            ),
            Err("ERR_MEDIA_RUNTIME_INTEGRITY")
        );
        let _ = std::fs::remove_file(path);
    }
}
