# Build from Source

## Prerequisites

- Node.js 20 or newer;
- Corepack and pnpm 9.15.0;
- stable Rust with `rustfmt` and `clippy`;
- Tauri 2 platform prerequisites;
- a verified FFmpeg/FFprobe candidate matching `apps/desktop/src-tauri/media-runtime.lock` for native media processing.

For macOS development, install Xcode command-line tools. Real-time microphone and system-audio testing requires the corresponding macOS permissions.

## Install and verify

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm i18n:validate
pnpm build
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
node tools/audit-public-release.mjs
```

## Stage the bundled media runtime

FFmpeg and FFprobe binaries are not committed to the repository and are never resolved from the user's shell `PATH`. Before native media processing or a release build, stage a verified candidate that matches the committed lock:

```bash
pnpm media-runtime:stage /absolute/path/to/verified/runtime_candidate
```

The staging command checks both SHA-256 values and copies the target-suffixed sidecars into the Git-ignored `apps/desktop/src-tauri/binaries/` directory. See [Bundled Media Runtime](BUNDLED_MEDIA_RUNTIME.md).

## Start the native application

```bash
pnpm tauri dev
```

The Tauri command builds the macOS system-audio sidecar when running on macOS. Generated sidecar and media-runtime files are intentionally ignored by Git. Without a staged verified media runtime, development compilation can proceed with fail-closed stubs, but uploaded media processing remains unavailable.

## Browser-only Mock demonstration

```bash
pnpm dev
```

This mode is an explicitly marked UI demonstration. It does not reproduce the encrypted native vault, provider credential handling, native audio capture, persistent storage, or native export boundary.

## Developer diagnostics and MockProvider

`MockProvider` is included in the open-source codebase for deterministic tests, offline QA, community development, and API-token cost control. It is not a production AI provider and is hidden from the normal public release UI by default. The Developer diagnostics build also shows the Test Provider Adapter, which is UI-extension-test-only and cannot process meetings.

Enable Developer diagnostics explicitly during native development when you need MockProvider scenarios such as timeout, unavailable provider, quota failure, authentication failure, or unsupported capability:

```bash
VITE_ACCORDMESH_ENABLE_DEV_TOOLS=1 pnpm tauri dev
```

To build a diagnostic binary with the MockProvider and Test Provider Adapter UI visible:

```bash
VITE_ACCORDMESH_ENABLE_DEV_TOOLS=1 pnpm tauri build
```

Do not set this flag for normal public DMG builds.

## OpenAI configuration

Do not place API keys in `.env`, source code, shell history, screenshots, issues, or test fixtures. Configure the key through the application's encrypted Provider Settings UI. Use a dedicated project key with restricted permissions and a small budget for development.

## Clean build expectations

A release candidate should build from a fresh clone without `node_modules`, `.pnpm-store`, `target`, generated sidecars, databases, or local application data copied from another machine.
