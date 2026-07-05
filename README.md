# AccordMesh

AccordMesh is an open-source, local-first desktop assistant for chronological meeting understanding. It is designed for both multilingual meetings and same-language conversations where participants may still misunderstand intent, constraints, decisions, or follow-up needs.

> **Release status:** `0.1.0-alpha.1` Developer Preview. The source is available for review and contribution. A signed and notarized macOS binary is not yet provided.

## What it does

AccordMesh can:

- keep the original chronological transcript as the primary evidence;
- provide optional text translation without replacing the source text;
- generate segment-level meaning, ambiguity, and response guidance;
- generate whole-meeting analysis, communication feedback, draft minutes, and recording comparison;
- capture microphone and macOS system audio as separate tracks during real-time assistance;
- import audio, video, transcript, and subtitle material for post-meeting analysis;
- preserve immutable artifact versions and controlled regeneration provenance;
- store sensitive meeting payloads, provider credentials, and managed media locally with encryption at rest;
- export selected results as Markdown, TXT, or JSON.

AccordMesh does **not** claim that AI output is complete or correct. Important conclusions, commitments, dates, owners, and decisions must be reviewed against the source evidence.

## Privacy model

AccordMesh is local-first, not offline-only.

- Sensitive meeting payloads, provider credentials, managed media, transcripts, and generated artifacts are encrypted at rest under the application password and key hierarchy.
- Operational metadata such as project titles, original file names, timestamps, status values, provider/model identifiers, local storage references, and database relationships is stored locally in SQLite and is not fully encrypted in this Developer Preview. Use non-sensitive titles and file names when their disclosure would be a concern.
- Provider credentials remain in the Rust/native layer and are not returned to the React UI.
- When a user invokes an external AI provider, the audio or text required for that request is sent to the selected provider under that provider's terms and retention policy.
- Real-time audio segments may be stored temporarily as encrypted local spool chunks for crash recovery, retry, and background finalization. Plaintext conversion files are deleted after encryption.
- Plaintext files exported by the user are outside the encrypted vault and are not removed by Reset Vault.

See [Privacy and Security Notes](docs/PRIVACY_AND_SECURITY_NOTES.md) and [Data and Security Model](docs/DATA_AND_SECURITY_MODEL.md).

## Bring your own provider credentials

AccordMesh uses a BYOK (bring your own key) model. Users configure their own provider credentials and are responsible for provider charges.

The architecture is provider-neutral. The current source contains:

- `OpenAIProvider`, behind the common provider interfaces;
- `MockProvider`, for deterministic offline development, automated tests, manual QA, community contribution workflows, and API-token cost control;
- `Test Provider Adapter`, a Developer-diagnostics-only UI extension test adapter that verifies provider registration, task model display, and capability rendering without processing meetings.

`MockProvider` is not a production AI provider. It does not call external AI services and is hidden from normal public release UI by default. Community developers can explicitly enable Developer diagnostics and the MockProvider UI with:

```bash
VITE_ACCORDMESH_ENABLE_DEV_TOOLS=1 pnpm tauri dev
```

Diagnostic builds can also be created explicitly with:

```bash
VITE_ACCORDMESH_ENABLE_DEV_TOOLS=1 pnpm tauri build
```

Normal public DMG builds should be created without this flag so that MockProvider, Test Provider Adapter, Mock scenario, and other testing controls are not shown to end users.

### Adding a Provider Adapter

AccordMesh's AI Provider UI is registry-driven. New providers should be added by defining a Provider Adapter rather than hardcoding labels or task-model rows in the Settings page.

A Provider Adapter should declare its display name, capabilities, task-model mapping, settings requirements, runtime behavior, validation, and error mapping. The Test Provider Adapter is included only in Developer diagnostics mode to verify that provider registration, task model display, and capability rendering work as expected. It fails closed with `ERR_TEST_PROVIDER_ADAPTER_UI_ONLY` and does not process meetings.

The OpenAI implementation and offline request/response contract tests are present. **Live OpenAI API smoke testing remains pending maintainer API access.** No real API key is included in this repository.

## Platform status

- **macOS:** primary validated development platform. Microphone capture and ScreenCaptureKit-based system-audio capture are implemented. System-audio capture may include sounds from other applications played by the Mac.
- **Windows:** shared architecture and microphone support exist, but the production WASAPI loopback system-audio backend and a complete Windows release are not yet available.
- **Linux:** not currently an officially supported release target.

See [Known Limitations](docs/KNOWN_LIMITATIONS.md) and [Validation Status](docs/VALIDATION_STATUS.md).

## Build from source

Prerequisites include Node.js 20 or newer, pnpm 9, the stable Rust toolchain, Tauri 2 platform prerequisites, and a verified FFmpeg/FFprobe candidate matching the repository runtime lock for native uploaded-media processing. macOS development also requires Xcode command-line tools and the relevant microphone and screen/system-audio permissions.

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm i18n:validate
pnpm build
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm tauri dev
```

`pnpm dev` starts an explicitly marked browser-only Mock demonstration. It does not create the encrypted native vault, accept provider credentials, persist real meeting content, or provide native audio and export behavior.

Detailed instructions are in [Build from Source](docs/BUILD_FROM_SOURCE.md).

## Bundled media runtime

FFmpeg and FFprobe binaries are not committed to the source repository. Native development and release builds stage an audited candidate that matches `apps/desktop/src-tauri/media-runtime.lock`. Release mode uses only the sidecars inside the application bundle and does not fall back to `PATH`, Homebrew, or arbitrary executable locations. See [Bundled Media Runtime](docs/BUNDLED_MEDIA_RUNTIME.md) and [Build from Source](docs/BUILD_FROM_SOURCE.md).

## Recording and consent

Users are responsible for complying with applicable recording, privacy, workplace, and consent requirements. AccordMesh is not designed for covert recording. The application must visibly indicate when real-time assistance is active.

## Documentation

- [Product Scope](docs/PRODUCT_SCOPE.md)
- [System Architecture](docs/SYSTEM_ARCHITECTURE_AND_IMPLEMENTATION_PLAN.md)
- [AI Provider Architecture](docs/AI_PROVIDER_ARCHITECTURE.md)
- [Provider Extension Guide](docs/PROVIDER_EXTENSION_GUIDE.md)
- [Real-Time and Media Pipelines](docs/REALTIME_AND_MEDIA_PIPELINES.md)
- [Translation Limitations](docs/TRANSLATION_LIMITATIONS.md)
- [Known Limitations](docs/KNOWN_LIMITATIONS.md)
- [Roadmap](ROADMAP.md)

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md) before submitting changes. Security issues should be reported privately as described in [SECURITY.md](SECURITY.md), not through a public issue.

## License

AccordMesh source code is licensed under the [Apache License 2.0](LICENSE). Third-party components remain under their respective licenses; see [Third-Party Notices](THIRD_PARTY_NOTICES.md), the [Dependency License Review](docs/DEPENDENCY_LICENSE_REVIEW.md), and the lockfiles.
