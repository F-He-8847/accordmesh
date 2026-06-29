# AccordMesh Domain Model and Storage

## 1. One meeting, one project

`MeetingProject` represents one real-world meeting.

A real-time online or in-person session creates the project immediately. If the user later uploads the corresponding recording, the media and derived analyses attach to the same project.

A new `upload_only` project is created only when the user starts from uploaded material without an earlier real-time project.

## 2. Core entities

### 2.1 MeetingProject

```ts
interface MeetingProject {
  id: string;
  title: string;
  origin: "realtime_online" | "realtime_in_person" | "upload_only";
  status: "active" | "completed" | "processing" | "failed";
  createdAt: string;
  updatedAt: string;
  realtimeSessionId?: string;
  mediaAssetIds: string[];
  timelineSegmentIds: string[];
  artifactIds: string[];
  generationRunIds: string[];
}
```

### 2.2 RealtimeSession

```ts
interface RealtimeSession {
  id: string;
  projectId: string;
  mode: "online" | "in_person";
  startedAt: string;
  endedAt?: string;
  status: "starting" | "running" | "paused" | "completed" | "interrupted";
  inputTracks: AudioTrackConfig[];
}
```

### 2.3 TrackRole

```ts
type TrackRole =
  | "remote_system_audio"
  | "local_microphone"
  | "room_microphone"
  | "uploaded_media"
  | "unknown";
```

### 2.4 TimelineSegment

```ts
interface TimelineSegment {
  id: string;
  projectId: string;
  sourceId: string;
  trackRole: TrackRole;
  startMs: number;
  endMs: number;
  sourceTranscript: string;
  detectedLanguage?: string;
  transcriptStatus: "partial" | "final" | "failed";
  confidence?: number;
  warnings: Array<
    "overlapping_speech" | "low_volume" | "high_noise" | "incomplete"
  >;
  createdAt: string;
}
```

Only finalized segments are durable evidence. Partial transcript may be held temporarily and optionally persisted for crash recovery but must not be treated as final analysis evidence.

### 2.5 MediaAsset

```ts
interface MediaAsset {
  id: string;
  projectId: string;
  kind: "audio" | "video" | "transcript" | "subtitle";
  originalFileName: string;
  importedAt: string;
  durationMs?: number;
  sha256: string;
  encryptedPath: string;
  processingStatus: "importing" | "ready" | "failed" | "deleted";
}
```

### 2.6 AnalysisArtifact

```ts
interface AnalysisArtifact {
  id: string;
  projectId: string;
  type:
    | "literal_translation"
    | "segment_understanding"
    | "meeting_context_snapshot"
    | "post_meeting_analysis"
    | "communication_review"
    | "intelligent_comparison_report"
    | "meeting_minutes";
  sourceIds: string[];
  schemaVersion: string;
  promptVersion: string;
  providerId: string;
  modelId: string;
  appVersion: string;
  createdAt: string;
  status: "queued" | "running" | "completed" | "failed";
  encryptedPayloadPath: string;
}
```

### 2.7 GenerationRun

Tracks one attempt, including error details and request metadata without storing secrets.

### 2.8 EvidenceRef

```ts
interface EvidenceRef {
  sourceId: string;
  segmentId?: string;
  startMs?: number;
  endMs?: number;
  evidenceType: "explicit_statement" | "contextual_support" | "model_inference";
  confidence: "low" | "medium" | "high";
}
```

## 3. Artifact immutability

Generated content is immutable. Regeneration creates a new `AnalysisArtifact` and new `GenerationRun`.

The UI may select a latest/current artifact for display but must preserve earlier completed versions.

## 4. Storage layout

Recommended local layout:

```text
app-data/
├── app.sqlite
├── vault/
├── projects/
│   └── {project-id}/
│       ├── manifest.enc
│       ├── realtime/
│       ├── media/
│       ├── transcripts/
│       ├── artifacts/
│       └── exports/
├── temp/
└── logs/
```

SQLite stores operational metadata, paths, states, and relationships. In the current Developer Preview, records such as project titles, original file names, timestamps, status values, provider/model identifiers, and local storage references are local but not fully encrypted. Sensitive meeting payloads, provider credentials, generated artifacts, and managed media are encrypted according to the security design.

## 5. Search scope

First version search:

- project title;
- date;
- origin;
- status.

Full-text search of transcripts is deferred.

## 6. Deletion

Project deletion must remove:

- project metadata;
- related session records;
- media assets;
- transcript payloads;
- artifacts;
- cached files;
- associated encryption keys;
- queued jobs that cannot be safely completed.

Deletion is irreversible in the first official version.
