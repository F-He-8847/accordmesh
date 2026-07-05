# Dependency License Review

This document summarizes the dependency-license inventory generated from the exact lockfiles used for the AccordMesh `0.1.0-alpha.1` Developer Preview source revision.

## Reviewed lockfiles

| Lockfile | SHA-256 |
|---|---|
| `pnpm-lock.yaml` | `bf8f0bbba71eafa6b067abab5eb003e2b03af31ce933c03edf659454b284ea1c` |
| `apps/desktop/src-tauri/Cargo.lock` | `2a2f26b4651a05f8393314e664cea86a556fad0904d5387d58378647ebc5c147` |

Review date: 2026-06-29

## Inventory result

The generated inventory contained:

- 680 dependency records in total;
- 608 Rust/Cargo records;
- 72 JavaScript/npm records;
- 0 records with missing license metadata.

No dependency was reported with GPL-only or AGPL-only metadata. License expressions that mention LGPL also offer MIT or Apache-2.0 alternatives in the recorded package metadata.

## License families requiring explicit awareness

The following packages are not release blockers for this source snapshot, but their notices and license terms must remain part of release review:

### MPL-2.0

- `cssparser` 0.36.0
- `cssparser-macros` 0.6.1
- `dtoa-short` 0.3.5
- `option-ext` 0.2.0
- `selectors` 0.36.1

These are separate, unmodified dependencies in the reviewed source revision. Any future vendoring or modification of MPL-covered files requires a fresh review.

### CC-BY-4.0

- `caniuse-lite` 1.0.30001799

This package contains browser-support data. Its attribution and license notice must be preserved when a release artifact redistributes that data.

### CDLA-Permissive-2.0

- `webpki-roots` 1.0.8

This package contains certificate-root data distributed under the recorded permissive data license.

### Unicode-3.0

The Rust dependency graph includes Unicode/ICU data and support crates under the Unicode-3.0 license. Required license text and attribution must be retained in binary-distribution notices.

### Other permissive licenses

The remaining inventory primarily uses MIT, Apache-2.0, BSD, ISC, Zlib, Unlicense, BSL-1.0, LLVM-exception, CC0-1.0, MIT-0, and compatible multi-license expressions.

## FFmpeg and platform SDKs

The source repository does not commit FFmpeg or FFprobe binaries. The Apple Silicon installable-release pipeline uses a separately audited FFmpeg 8.1.2 candidate built under an LGPL 2.1-or-later configuration with `CONFIG_GPL=0` and `CONFIG_NONFREE=0`. The native runtime is staged from private release evidence, checked against the committed unsigned-runtime lock, and packaged as application sidecars. Final signed-binary hashes and LGPL compliance materials must be regenerated for each binary release.

Apple platform SDK files, signing identities, provisioning profiles, and private keys are not included in the repository.

## Release rule

This review applies only to the lockfile versions and source snapshot identified above. Dependency upgrades, vendoring, bundled native binaries, or a new binary distribution require regeneration and review of the inventory.

The generated machine-local CSV inventory is release evidence and is intentionally kept outside the public source tree because it contains package-manager cache paths. The public repository keeps this normalized review instead.
