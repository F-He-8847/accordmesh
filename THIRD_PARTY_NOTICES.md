# Third-Party Notices

AccordMesh depends on open-source packages from the Rust and JavaScript ecosystems. Each dependency remains subject to its own license and copyright notices.

The authoritative dependency versions for this source release are recorded in:

- `apps/desktop/src-tauri/Cargo.lock`
- `pnpm-lock.yaml`
- the corresponding `Cargo.toml` and `package.json` manifests

Primary direct dependency families include Tauri, React, Vite, TypeScript, SQLite through `rusqlite`, Argon2, AES-GCM, Tokio, Reqwest, CPAL, Serde, and related transitive packages.

The normalized review for the current lockfiles is available in [Dependency License Review](docs/DEPENDENCY_LICENSE_REVIEW.md). The review records 680 dependency entries with no missing license metadata and identifies the MPL-2.0, CC-BY-4.0, CDLA-Permissive-2.0, Unicode-3.0, and other attribution-sensitive entries that require continuing release attention.

## Browser-support data

The JavaScript dependency graph includes `caniuse-lite`, whose browser-support data is recorded as CC-BY-4.0. Required attribution and license terms must be preserved when redistributing that data.

## Unicode and certificate-root data

The Rust dependency graph includes Unicode/ICU components under Unicode-3.0 and certificate-root data through `webpki-roots` under CDLA-Permissive-2.0. Binary-distribution notices must preserve the applicable terms.

## FFmpeg and FFprobe

FFmpeg and FFprobe binaries are not committed to this source repository. Installable macOS release construction may bundle a separately audited FFmpeg build as application sidecars. The approved Apple Silicon runtime is built from FFmpeg 8.1.2 official source under an LGPL 2.1-or-later configuration with GPL and nonfree components disabled. Each binary release must publish or reference the exact build information, corresponding source materials, hashes, notices, and LGPL compliance artifacts for the bytes it distributes.

## Platform SDKs

The macOS system-audio helper uses Apple's ScreenCaptureKit and platform SDKs supplied by the local Xcode installation. Apple SDK files, signing certificates, provisioning profiles, and private keys are not included in this repository.

## Release audit

Before distributing a binary, maintainers must regenerate and review the complete dependency-license inventory, preserve required notices, create an SBOM, and resolve any missing, custom, strong-copyleft, or otherwise incompatible license metadata. See [Dependency License Policy](docs/DEPENDENCY_LICENSE_POLICY.md).
