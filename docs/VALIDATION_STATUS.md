# Validation Status

## Public source checks

The public repository includes a self-contained contract test suite covering representative security and business invariants. Before each public release, maintainers should run:

```bash
pnpm install --frozen-lockfile
pnpm i18n:validate
pnpm build
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
node tools/audit-public-release.mjs
```

## Public source validation — 2026-06-29

The `0.1.0-alpha.1` public source revision was prepared for validation from an isolated copy on macOS. The release checks cover:

- public-tree sensitive-content and forbidden-file audit;
- JavaScript installation from the frozen lockfile;
- i18n resource validation;
- TypeScript checking and Vite production build;
- Rust formatting and `cargo check --locked`;
- 24 self-contained public Rust tests;
- dependency-license inventory containing 680 records and no missing license metadata;
- source-tree immutability during isolated validation.

The private GitHub publication rehearsal also confirmed that a fresh clone, the tracked Git tree, and GitHub's generated source archive matched the corresponding local commit, and that the archive passed the public-tree sensitive-content audit. The exact tagged revision must be rescanned before a release is published.

## Validated product areas

Automated and macOS manual validation has covered:

- vault creation, unlock, lock, restart, and Reset Vault lifecycle;
- encrypted provider configuration and API-key masking;
- local library, rename, deletion guards, immutable artifact versions, and export;
- upload processing, media normalization, long-media chunking, cancellation, retry, and cleanup;
- microphone and macOS system-audio capture;
- real-time encrypted spool, restart recovery, provider-independent Stop, and finalization;
- overlay behavior, selected-version display, controlled regeneration, history, and technical provenance;
- deterministic MockProvider flows and offline OpenAI request/response contracts.

## Not yet validated as a public release claim

- live OpenAI API text, structured-output, and transcription behavior;
- signed/notarized macOS packaging and Gatekeeper distribution;
- a complete Windows release and WASAPI loopback implementation;
- binary packaging that bundles FFmpeg;
- clean builds on every potential platform target.

Validation describes tested behavior. It is not a guarantee that the software is error-free or suitable for a particular high-stakes use.
