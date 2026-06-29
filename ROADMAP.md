# AccordMesh Roadmap

The roadmap describes direction, not a delivery commitment.

## Completed for the initial public-source preview

- Audit the public source tree for credentials, personal paths, runtime data, and release-only artifacts.
- Review dependency-license metadata from the exact JavaScript and Rust lockfiles.
- Validate frozen dependency installation, i18n resources, the frontend production build, Rust formatting, compilation, and the public Rust test suite on macOS.
- Add reproducible continuous-integration checks and public validation documentation.

## Provider validation

- Run minimal live OpenAI text, structured-output, and transcription smoke tests with fictional data and a restricted maintainer key.
- Document validated model combinations, error behavior, cancellation, timeouts, and cost boundaries.
- Expand provider adapters while preserving capability, privacy, and provenance contracts.

## macOS preview distribution

- Review entitlements and permission copy.
- Produce a release build with Hardened Runtime.
- Add Developer ID signing and Apple notarization.
- Decide whether FFmpeg remains an external prerequisite or is distributed under a separately audited packaging plan.

## Cross-platform work

- Implement and validate Windows WASAPI loopback system-audio capture.
- Validate Windows vault, media, export, and lifecycle behavior.
- Evaluate Linux feasibility without weakening platform security boundaries.

## Privacy and local data protection

- Evaluate encryption or minimization of operational metadata such as project titles and original file names.
- Improve user controls for metadata-sensitive project naming and local retention.

## Community extensibility

- Add community UI locales through isolated i18n resources.
- Build reproducible meeting-understanding evaluation fixtures and scoring tools.
- Improve accessible UI, contributor documentation, and provider compatibility reporting.
