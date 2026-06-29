# AccordMesh System Architecture and Implementation Plan

## 1. Technical stack

Current Developer Preview stack:

- Tauri 2 desktop shell;
- React + TypeScript + Vite UI;
- Rust application core;
- SQLite operational metadata/job database;
- encrypted storage for sensitive meeting payloads, credentials, artifacts, and managed media;
- provider capability interfaces;
- MockProvider and OpenAIProvider implementations;
- platform adapters for macOS and Windows audio;
- controlled media-processing adapter for uploaded audio/video.

## 2. High-level architecture

```text
React / TypeScript UI
    |
    | Tauri commands and events
    v
Rust Application Core
    |- Authentication and local vault
    |- MeetingProject repository
    |- Persistent job queue
    |- Provider registry
    |- Real-time orchestrator
    |- Media ingestion
    |- Context manager
    |- Analysis pipeline
    |- Semantic comparison pipeline
    |- Export service
    |- Platform adapters
```

## 3. Repository layout

```text
accordmesh/
├── apps/
│   └── desktop/
│       ├── src/
│       │   ├── app/
│       │   ├── components/
│       │   ├── features/
│       │   ├── i18n/
│       │   └── shared/
│       └── src-tauri/
│           ├── src/
│           │   ├── auth/
│           │   ├── crypto/
│           │   ├── storage/
│           │   ├── projects/
│           │   ├── audio/
│           │   ├── media/
│           │   ├── realtime/
│           │   ├── providers/
│           │   ├── context/
│           │   ├── analysis/
│           │   ├── comparison/
│           │   ├── jobs/
│           │   ├── export/
│           │   └── platform/
│           ├── migrations/
│           └── icons/
├── packages/
│   ├── contracts/
│   ├── schemas/
│   ├── prompts/
│   └── mock-fixtures/
├── evals/
├── docs/
├── tools/
└── .github/
```

## 4. Architectural boundaries

### 4.1 UI layer

Responsible for:

- pages and navigation;
- overlay;
- user interaction;
- i18n rendering;
- read-only artifact display;
- provider setup forms;
- progress and error states.

It must not:

- access raw provider credentials;
- implement encryption;
- call provider APIs directly;
- parse provider-specific responses;
- persist sensitive meeting content directly.

### 4.2 Rust application core

Responsible for:

- unlocking and vault state;
- encrypted storage;
- project repository;
- job orchestration;
- media processing;
- audio capture;
- provider calls;
- schema validation;
- artifact versioning;
- export and deletion.

### 4.3 Provider adapters

Responsible for:

- provider-specific authentication;
- request construction;
- streaming/event adaptation;
- error mapping;
- capability declaration;
- mapping responses into unified domain contracts.

They must not own business rules for meeting interpretation.

### 4.4 Analysis assets

Prompt instructions and JSON schemas are independent assets. Provider adapters consume them but do not redefine them.

## 5. Application state domains

- locked/unlocked vault state;
- provider configuration state;
- active meeting state;
- project library state;
- persistent job state;
- artifact generation state;
- platform permission/device state.

Keep these state domains separate.

## 6. Current implementation status

The Developer Preview includes:

- native setup, unlock, lock, and Reset Vault flows;
- encrypted provider credentials, sensitive meeting payloads, generated artifacts, and managed media;
- local SQLite operational metadata that is not fully encrypted in the current Developer Preview;
- local project library and project detail views;
- deterministic MockProvider end-to-end flows;
- provider registry and OpenAIProvider adapter boundary;
- upload/import, media normalization, chunking, and retryable processing;
- macOS microphone and system-audio capture;
- chronological timeline and evidence provenance;
- translation, segment insight, meeting analysis, communication feedback, comparison, and minutes artifacts;
- persistent jobs, immutable versions, controlled regeneration, export, and deletion;
- English i18n resources and community extension points.

Live OpenAI API smoke testing, signed/notarized macOS packaging, and the Windows WASAPI loopback implementation remain outside the validated Developer Preview scope. See `docs/KNOWN_LIMITATIONS.md`.
