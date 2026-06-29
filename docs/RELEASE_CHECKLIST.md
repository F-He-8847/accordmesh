# Release Checklist

This checklist applies to every public source or binary release.

## Source and history

- [ ] The release is built from the audited public repository and the exact tagged revision.
- [ ] `node tools/audit-public-release.mjs` passes.
- [ ] Secret scanning covers the current tree and all public Git history.
- [ ] No environment files, credentials, vaults, databases, media, logs, archives, or personal paths are tracked.
- [ ] Markdown links and repository metadata point to existing public resources.

## Build and tests

- [ ] `pnpm install --frozen-lockfile` succeeds from a clean clone.
- [ ] `pnpm i18n:validate` passes.
- [ ] `pnpm build` passes.
- [ ] `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check` passes.
- [ ] `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml` passes.
- [ ] `cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml` passes.
- [ ] Platform-specific manual checks use fictional data and an isolated local vault.

## Security and privacy

- [ ] Permission copy and privacy documentation match the implementation.
- [ ] External-provider data flow and plaintext export boundaries are clearly disclosed.
- [ ] Security reporting is enabled through GitHub Private Vulnerability Reporting.
- [ ] Real provider smoke tests, if any, use a restricted key, fictional data, and a small budget.

## Licensing and assets

- [ ] Apache-2.0 metadata is consistent across the repository.
- [ ] Dependency-license inventory is generated from the exact lockfiles and reviewed.
- [ ] Required third-party notices and an SBOM are included for binary releases.
- [ ] FFmpeg packaging obligations are reviewed if any binary is bundled.
- [ ] Icons, screenshots, sample data, and other assets have documented provenance.

## GitHub release

- [ ] The exact Git tag and generated source archive are rescanned.
- [ ] Release notes identify Developer Preview status and known limitations.
- [ ] Unsupported platforms and untested providers are not presented as complete.
- [ ] The release contains no unsigned binary unless its installation and security limitations are explicit.
