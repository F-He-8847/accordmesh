# Bundled Media Runtime

AccordMesh desktop release builds use a bundled FFmpeg/FFprobe runtime for uploaded audio and video processing. The application does not search the user's shell `PATH`, Homebrew directories, or arbitrary user-selected executable paths in release mode.

## Release boundary

On macOS, Tauri packages the approved sidecars into:

```text
AccordMesh.app/Contents/MacOS/ffmpeg
AccordMesh.app/Contents/MacOS/ffprobe
```

The release application accepts the runtime only when:

- the main executable is running from an `.app/Contents/MacOS` directory;
- both sidecars are sibling files in that directory;
- both files match the SHA-256 values compiled into the application from the selected runtime lock;
- both tools report the expected FFmpeg version through the existing bounded process runner.

Failure is closed. Release mode does not fall back to `ffmpeg` or `ffprobe` on `PATH`, `/opt/homebrew`, `/usr/local`, a source checkout, or an environment-supplied runtime directory.

## Source-tree boundary

Third-party FFmpeg binaries are intentionally not committed to this source repository. The ignored directory below is populated only for local development and release construction:

```text
apps/desktop/src-tauri/binaries/
```

The repository records the approved unsigned runtime identity in:

```text
apps/desktop/src-tauri/media-runtime.lock
```

The lock identifies the expected version, target triple, minimum macOS version, license path, and SHA-256 values. A verified local candidate can be staged with:

```bash
pnpm media-runtime:stage /absolute/path/to/verified/runtime_candidate
```

The staging command verifies both source files against the lock before copying them atomically into the ignored binaries directory.

## Development and test behavior

Debug builds use only the target-suffixed files in the local ignored binaries directory, or an explicit `ACCORDMESH_DEV_MEDIA_RUNTIME_DIR` supplied by the developer. The same SHA-256 checks apply.

Unit tests may use the dedicated `ACCORDMESH_TEST_FFMPEG_BIN` and `ACCORDMESH_TEST_FFPROBE_BIN` overrides. These test-only variables are not a release fallback.

When no verified runtime has been staged, the Rust build script may create development-only executable stubs so formatting, compilation, and tests can proceed without silently using a machine-global FFmpeg installation. A release build fails if the required verified runtime is absent.

## Signing and notarization

Apple code signing changes executable bytes. Release construction therefore supports selecting a generated lock through `ACCORDMESH_MEDIA_RUNTIME_LOCK_FILE`. The generated lock must describe the exact sidecar bytes included in that release build. This keeps runtime integrity verification compatible with nested-code signing without changing the committed unsigned provenance lock.

The signed lock is release evidence and remains outside the public source tree unless a sanitized release manifest is intentionally published.

## Supply-chain and license record

The current Apple Silicon candidate is built from FFmpeg 8.1.2 official source under an LGPL 2.1-or-later configuration with GPL and nonfree components disabled. Source-signature verification, configure output, binary hashes, dependency inspection, and media-format validation are maintained as release evidence outside the source repository.

Every binary release that includes FFmpeg/FFprobe must also provide the corresponding notices, exact build information, source or source-retrieval materials, and required LGPL compliance artifacts.
