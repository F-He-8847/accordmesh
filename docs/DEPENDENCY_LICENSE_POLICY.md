# Dependency License Policy

AccordMesh accepts dependencies only when their licensing and distribution obligations are compatible with the project's intended source and binary releases.

## Review requirements

For every release candidate:

1. inventory direct and transitive Rust and JavaScript dependencies from the lockfiles;
2. record each package name, version, source, SPDX expression, and notice file;
3. investigate missing, custom, deprecated, or ambiguous license metadata;
4. separately review strong-copyleft, network-copyleft, noncommercial, source-available, or nonfree terms;
5. preserve required attribution and NOTICE text;
6. produce an SBOM for binary releases;
7. review bundled native tools and platform helpers independently.

Unknown licenses fail the release review until resolved.

## FFmpeg

FFmpeg and FFprobe binaries are not committed to the source repository. Installable release construction may bundle a separately audited runtime only when maintainers record the official source version and signature, exact configure output, target, binary hashes, linked dependencies, notices, and corresponding source materials. The approved release path must keep GPL and nonfree components disabled unless a separate legal and product review explicitly changes that decision.

## Assets

Icons, screenshots, fonts, sample media, and documentation excerpts require provenance records. See [Asset Provenance](ASSET_PROVENANCE.md).

## Generated inventory

A generated inventory is release evidence, not hand-maintained source. It should be generated from the exact release lockfiles, reviewed outside the source tree, and only the final required notice/SBOM artifacts should be committed.
