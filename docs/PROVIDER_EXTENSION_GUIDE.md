# Provider Extension Guide

Provider adapters live under `apps/desktop/src-tauri/src/providers` and implement the capability traits in `providers/mod.rs`: file and real-time transcription, translation, segment understanding, meeting synthesis, communication review, comparison, and minutes. Register definitions and resolution in `providers/registry.rs`; general commands must never construct an adapter directly.

Declare only capabilities with real execution paths. Resolve credentials through `Repository::provider_configuration`; never return secrets to React or include them in logs, errors, artifacts, or exports. Provider-specific request/response types remain inside the adapter and must map to `TranscriptDraft` or `GeneratedDraft` before orchestration.

Consume versioned prompts from `packages/prompts` and matching schemas from `packages/schemas`. Every attempt must create a `GenerationRun`; validate payloads before immutable artifact persistence and record `ERR_PROVIDER_SCHEMA` on invalid output. Map transport, authentication, quota, timeout, availability, parsing, and capability failures to stable AccordMesh codes.

Document supported formats/languages, data retention behavior, network endpoints, pricing risk, and quality limitations in the provider definition or extension documentation.
