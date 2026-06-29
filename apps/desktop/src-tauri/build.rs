use std::env;
use std::fs;
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
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_macos_system_audio_helper(&target);
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
