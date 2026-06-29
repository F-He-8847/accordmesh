# Contributing to AccordMesh

Thank you for helping improve AccordMesh.

## Before you start

Please read the relevant public architecture documents:

- [System Architecture](docs/SYSTEM_ARCHITECTURE_AND_IMPLEMENTATION_PLAN.md)
- [AI Provider Architecture](docs/AI_PROVIDER_ARCHITECTURE.md)
- [Data and Security Model](docs/DATA_AND_SECURITY_MODEL.md)
- [Internationalization Architecture](docs/I18N_ARCHITECTURE.md)
- [Analysis Contracts](docs/ANALYSIS_CONTRACTS.md)

For substantial changes, open an issue first so the problem, security boundary, and compatibility impact can be discussed before implementation.

## Architectural rules

Contributions must preserve these boundaries:

- provider credentials remain in the Rust/native layer;
- the UI never stores or logs raw credentials;
- provider-specific request and response types stay inside provider adapters;
- source transcript, translation, inference, ambiguity, and guidance remain separate;
- generated artifacts are immutable versions with provenance;
- tests use fictional data and must not access a contributor's real vault, media, or credentials;
- no test may make a real provider call unless it is explicitly isolated, opt-in, and documented.

## User-visible text and i18n

English resources under `apps/desktop/src/i18n/locales/en/` are authoritative. Do not hard-code new user-visible text in business logic.

Run:

```bash
pnpm i18n:validate
```

See [I18n Contribution Guide](docs/I18N_CONTRIBUTION_GUIDE.md).

## Local checks

Before opening a pull request:

```bash
pnpm install --frozen-lockfile
pnpm i18n:validate
pnpm build
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
node tools/audit-public-release.mjs
```

Changes to media processing should also be tested with fictional, non-sensitive files. Do not include generated databases, vaults, audio, video, logs, API responses, or exported meeting content in a pull request.

## Pull requests

Keep pull requests focused. Explain:

- the user or maintainer problem being solved;
- files and architectural boundaries affected;
- security and privacy implications;
- tests performed;
- known limitations or follow-up work.

By contributing, you agree that your contribution is licensed under Apache License 2.0.
