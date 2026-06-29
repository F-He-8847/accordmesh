# Build from Source

## Prerequisites

- Node.js 20 or newer;
- Corepack and pnpm 9.15.0;
- stable Rust with `rustfmt` and `clippy`;
- Tauri 2 platform prerequisites;
- FFmpeg and FFprobe on `PATH` for uploaded media processing.

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

## Start the native application

```bash
pnpm tauri dev
```

The Tauri command builds the macOS system-audio sidecar when running on macOS. Generated sidecar files are intentionally ignored by Git.

## Browser-only Mock demonstration

```bash
pnpm dev
```

This mode is an explicitly marked UI demonstration. It does not reproduce the encrypted native vault, provider credential handling, native audio capture, persistent storage, or native export boundary.

## OpenAI configuration

Do not place API keys in `.env`, source code, shell history, screenshots, issues, or test fixtures. Configure the key through the application's encrypted Provider Settings UI. Use a dedicated project key with restricted permissions and a small budget for development.

## Clean build expectations

A release candidate should build from a fresh clone without `node_modules`, `.pnpm-store`, `target`, generated sidecars, databases, or local application data copied from another machine.
