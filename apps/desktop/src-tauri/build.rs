use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

const HELPER_BUILD_CACHE_VERSION: &str = "accordmesh-system-audio-v2";
const HELPER_BUILD_FLAGS: &str = "-O|-parse-as-library|-framework|Foundation|-framework|ScreenCaptureKit|-framework|CoreMedia|-framework|CoreAudio|-framework|CoreGraphics";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/platform/macos/system_audio.swift");
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    let target = env::var("TARGET").expect("TARGET is available to build scripts");
    println!("cargo:rustc-env=ACCORDMESH_TARGET_TRIPLE={target}");
    println!("cargo:rerun-if-env-changed=ACCORDMESH_MEDIA_RUNTIME_LOCK_FILE");
    let media_runtime_lock = load_media_runtime_lock();
    emit_media_runtime_environment(&media_runtime_lock);
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_macos_system_audio_helper(&target);
        prepare_macos_media_runtime_sidecars(&target, &media_runtime_lock);
    }
    tauri_build::build();
}

fn build_macos_system_audio_helper(target: &str) {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let source = manifest.join("src/platform/macos/system_audio.swift");
    let binary_dir = manifest.join("binaries");
    fs::create_dir_all(&binary_dir).expect("create sidecar directory");
    let output = binary_dir.join(format!("accordmesh-system-audio-{target}"));
    let stamp = binary_dir.join(format!(".accordmesh-system-audio-{target}.build-stamp"));
    let expected_stamp = helper_build_stamp(&source, target);

    if helper_is_current(&output, &stamp, &expected_stamp) {
        println!(
            "cargo:warning=AccordMesh system-audio sidecar is current; skipping Swift rebuild"
        );
        return;
    }

    let temporary_output = binary_dir.join(format!(
        ".accordmesh-system-audio-{target}.tmp-{}",
        process::id()
    ));
    let temporary_stamp = binary_dir.join(format!(
        ".accordmesh-system-audio-{target}.build-stamp.tmp-{}",
        process::id()
    ));
    let _ = fs::remove_file(&temporary_output);
    let _ = fs::remove_file(&temporary_stamp);

    let status = Command::new("xcrun")
        .args(["swiftc", "-O", "-parse-as-library"])
        .arg(&source)
        .args(["-o"])
        .arg(&temporary_output)
        .args([
            "-framework",
            "Foundation",
            "-framework",
            "ScreenCaptureKit",
            "-framework",
            "CoreMedia",
            "-framework",
            "CoreAudio",
            "-framework",
            "CoreGraphics",
        ])
        .status()
        .expect("xcrun swiftc is required to build the macOS system-audio helper");
    if !status.success() {
        let _ = fs::remove_file(&temporary_output);
        panic!("failed to compile macOS system-audio helper");
    }

    fs::write(&temporary_stamp, &expected_stamp).expect("write sidecar build stamp");
    fs::rename(&temporary_output, &output).expect("atomically install system-audio helper");
    fs::rename(&temporary_stamp, &stamp).expect("atomically install sidecar build stamp");
}

fn helper_is_current(output: &Path, stamp: &Path, expected_stamp: &str) -> bool {
    if !output.is_file() {
        return false;
    }
    fs::read_to_string(stamp)
        .map(|actual| actual == expected_stamp)
        .unwrap_or(false)
}

fn helper_build_stamp(source: &Path, target: &str) -> String {
    let source_hash = command_text(
        Command::new("shasum").args(["-a", "256"]).arg(source),
        "shasum is required to fingerprint the macOS system-audio helper",
    )
    .split_whitespace()
    .next()
    .expect("shasum output contains a digest")
    .to_owned();
    let swift_version = command_text(
        Command::new("xcrun").args(["swiftc", "--version"]),
        "xcrun swiftc --version is required to fingerprint the helper toolchain",
    );
    format!(
        "{HELPER_BUILD_CACHE_VERSION}\nsource_sha256={source_hash}\ntarget={target}\nswiftc={swift_version}\nflags={HELPER_BUILD_FLAGS}\n"
    )
}

fn command_text(command: &mut Command, failure_message: &str) -> String {
    let output = command.output().expect(failure_message);
    if !output.status.success() {
        panic!("{failure_message}");
    }
    let mut value = String::from_utf8_lossy(&output.stdout).into_owned();
    value.push_str(&String::from_utf8_lossy(&output.stderr));
    value.replace("\r\n", "\n").trim().to_owned()
}

fn prepare_macos_media_runtime_sidecars(target: &str, lock: &HashMap<String, String>) {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let binary_dir = manifest.join("binaries");
    fs::create_dir_all(&binary_dir).expect("create media runtime sidecar directory");
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let lock_target = lock_value(lock, "target").expect("media runtime lock target");
    if profile == "release" && target != lock_target {
        panic!(
            "release build target {target} does not match media-runtime.lock target {lock_target}"
        );
    }

    for name in ["ffmpeg", "ffprobe"] {
        let path = binary_dir.join(format!("{name}-{target}"));
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_file() {
            if target == lock_target {
                let expected =
                    lock_value(lock, &format!("{name}_sha256")).expect("media runtime lock digest");
                let actual = file_sha256(&path);
                if actual != expected {
                    if profile != "release" && is_development_stub(&path, name) {
                        ensure_executable(&path);
                        println!(
                            "cargo:warning=Reusing the exact development-only {name} sidecar stub."
                        );
                        continue;
                    }
                    panic!(
                        "{name} sidecar SHA-256 does not match media-runtime.lock: expected {expected}, got {actual}"
                    );
                }
            }
            ensure_executable(&path);
            continue;
        }

        if profile == "release" {
            panic!(
                "release build requires staged bundled media runtime: {}",
                path.display()
            );
        }
        write_development_stub(&path, name);
        println!(
            "cargo:warning=Created a development-only {name} sidecar stub. Stage the verified runtime before media processing or release builds."
        );
    }
}

fn load_media_runtime_lock() -> HashMap<String, String> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let path = env::var_os("ACCORDMESH_MEDIA_RUNTIME_LOCK_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("media-runtime.lock"));
    println!("cargo:rerun-if-changed={}", path.display());
    let text = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read media runtime lock {}: {error}",
            path.display()
        )
    });
    let values = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (key, value) = line
                .split_once('=')
                .unwrap_or_else(|| panic!("invalid media runtime lock line: {line}"));
            (key.trim().to_string(), value.trim().to_string())
        })
        .collect::<HashMap<_, _>>();
    if lock_value(&values, "format_version").as_deref() != Some("1") {
        panic!("unsupported media runtime lock format");
    }
    for key in [
        "ffmpeg_version",
        "target",
        "minimum_macos",
        "license",
        "ffmpeg_sha256",
        "ffprobe_sha256",
    ] {
        if !values.contains_key(key) {
            panic!("media runtime lock is missing {key}");
        }
    }
    for key in ["ffmpeg_sha256", "ffprobe_sha256"] {
        let digest = lock_value(&values, key).expect("validated media runtime digest");
        if digest.len() != 64 || !digest.bytes().all(|value| value.is_ascii_hexdigit()) {
            panic!("media runtime lock contains an invalid {key}");
        }
    }
    if lock_value(&values, "license").as_deref() != Some("LGPL-2.1-or-later") {
        panic!("media runtime lock contains an unsupported license path");
    }
    values
}

fn emit_media_runtime_environment(lock: &HashMap<String, String>) {
    for (key, env_name) in [
        ("ffmpeg_version", "ACCORDMESH_MEDIA_RUNTIME_VERSION"),
        ("target", "ACCORDMESH_MEDIA_RUNTIME_TARGET"),
        ("minimum_macos", "ACCORDMESH_MEDIA_RUNTIME_MINIMUM_MACOS"),
        ("license", "ACCORDMESH_MEDIA_RUNTIME_LICENSE"),
        ("ffmpeg_sha256", "ACCORDMESH_MEDIA_RUNTIME_FFMPEG_SHA256"),
        ("ffprobe_sha256", "ACCORDMESH_MEDIA_RUNTIME_FFPROBE_SHA256"),
    ] {
        let value = lock_value(lock, key).expect("validated media runtime lock value");
        println!("cargo:rustc-env={env_name}={value}");
    }
}

fn lock_value(lock: &HashMap<String, String>, key: &str) -> Option<String> {
    lock.get(key).cloned()
}

fn file_sha256(path: &Path) -> String {
    command_text(
        Command::new("shasum").args(["-a", "256"]).arg(path),
        "shasum is required to verify bundled media runtime",
    )
    .split_whitespace()
    .next()
    .expect("shasum output contains a digest")
    .to_owned()
}

fn development_stub_contents(name: &str) -> String {
    format!(
        "#!/bin/sh\necho 'AccordMesh verified bundled {name} runtime is not staged' >&2\nexit 127\n"
    )
}

fn is_development_stub(path: &Path, name: &str) -> bool {
    fs::read_to_string(path)
        .map(|actual| actual == development_stub_contents(name))
        .unwrap_or(false)
}

fn write_development_stub(path: &Path, name: &str) {
    let temporary = path.with_extension(format!("stub-{}", process::id()));
    let _ = fs::remove_file(&temporary);
    let mut file = fs::File::create(&temporary).expect("create development media runtime stub");
    file.write_all(development_stub_contents(name).as_bytes())
        .expect("write development media runtime stub");
    ensure_executable(&temporary);
    fs::rename(&temporary, path).expect("atomically install development media runtime stub");
}

#[cfg(unix)]
fn ensure_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .expect("read media runtime permissions")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("set media runtime executable permissions");
}

#[cfg(not(unix))]
fn ensure_executable(_path: &Path) {}
