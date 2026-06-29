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

The current source repository invokes user-installed FFmpeg/FFprobe and does not bundle an FFmpeg binary. If a future installer bundles FFmpeg, maintainers must record the exact build configuration and satisfy the corresponding LGPL/GPL obligations. Builds using nonfree components must not be distributed without a separate legal review.

## Assets

Icons, screenshots, fonts, sample media, and documentation excerpts require provenance records. See [Asset Provenance](ASSET_PROVENANCE.md).

## Generated inventory

A generated inventory is release evidence, not hand-maintained source. It should be generated from the exact release lockfiles, reviewed outside the source tree, and only the final required notice/SBOM artifacts should be committed.
