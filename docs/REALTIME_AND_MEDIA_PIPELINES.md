# AccordMesh Real-Time and Media Pipelines

## 1. Online real-time mode

### Inputs

- remote/system audio;
- local microphone.

Process them as separate tracks. Do not identify individual remote speakers.

### Pipeline

```text
capture -> normalize -> VAD/segmentation -> realtime transcription
-> partial transcript -> final transcript -> save TimelineSegment
-> update context -> segment analysis -> save artifact -> update overlay
```

### Platform adapters

- macOS: ScreenCaptureKit system-audio helper and CPAL microphone adapter.
- Windows: WASAPI loopback system-audio backend contract and CPAL microphone adapter. The WASAPI production implementation is deferred until the Windows build stage.

Keep adapters behind common Rust traits and conditional compilation. Platform-native types must not leak into the realtime, project, Provider, or Artifact layers. Both platforms emit the same `AudioFrame` contract with an explicit `track_role`.

The first macOS implementation captures the Mac's system output rather than microphone playback. It is independent of whether the user listens through speakers or headphones. Until per-application/window filtering is implemented, the UI must disclose that other sounds played by the Mac may also be included.

## 2. In-person real-time mode

Input:

- selected room/laptop/USB microphone.

Before start:

- device selection;
- input-level meter;
- short sound check;
- low-volume/noise/clipping status;
- privacy notice.

Use neutral wording. Do not claim who spoke or who failed to respond.

## 3. Audio normalization

Normalize to provider-compatible internal frames with:

- track role;
- monotonic timestamps;
- fixed frame duration;
- mono format where required;
- explicit sample rate;
- backpressure handling.

Do not hard-code provider-specific format into the domain layer.

## 4. Segmentation

Use a combination of:

- voice activity detection;
- silence duration;
- maximum segment duration;
- transcript completeness;
- manual “Analyze now.”

Partial transcript is visually temporary. Final transcript becomes durable evidence.

## 5. Overlay

Independent always-on-top window with:

- core meaning;
- Answer/Explain/Ask/Confirm;
- view source;
- translate source;
- copy;
- pause;
- quick hide;
- size/font/transparency controls;
- remembered position.

Outdated analysis must not overwrite a newer active result.

## 6. Queue/backpressure

Priority:

1. audio capture;
2. source transcript;
3. final segment persistence;
4. newest relevant understanding;
5. translation;
6. older derived analysis.

When overloaded:

- limit concurrent calls;
- combine adjacent short segments when safe;
- save late results without interrupting the current overlay;
- never discard source transcript for speed.

## 7. Upload pipeline

```text
select file -> controlled temp copy -> hash -> format validation
-> audio extraction/normalization -> chunking -> encrypt/store
-> transcription job -> transcript merge -> analysis jobs
```

Support common audio/video/text/subtitle formats through an adapter. The exact supported list is configuration/documentation, not a hard-coded product claim.

## 8. Long media

Each chunk records:

- index;
- original offset;
- start/end;
- overlap;
- hash;
- state;
- retry count;
- transcript or error.

Preserve original timeline offsets when merging.

## 9. Same-project attachment

When a user uploads the recording for a real-time meeting, attach it to the existing `MeetingProject`, run post-meeting analysis, and generate a semantic comparison report. Do not create a duplicate project.

## 10. Durable real-time spool and Provider-independent Stop

Every finalized real-time audio segment must enter a durable encrypted spool before any remote Provider result is treated as authoritative. Live AI processing is a low-latency optimization; the encrypted spool and background finalization job are the reliability path.

```text
final audio segment
-> write temporary local WAV
-> serialize stable chunk metadata and WAV bytes as one envelope
-> encrypt the envelope with the project key
-> atomically commit one `.chunk.enc` file
-> delete plaintext WAV
-> optionally queue the encrypted chunk for the live Provider worker
```

The Provider worker decrypts a queued chunk only when needed, writes a controlled temporary WAV for the provider call, and removes that temporary file when the call finishes or fails. A successful live Provider call may persist the final transcript, translation, and segment understanding immediately. Stable segment, GenerationRun, and Artifact identities make replay idempotent. The encrypted chunk is removed only after all required outputs for that chunk have committed successfully.

Stopping local capture must not wait for a remote Provider request. The order is:

```text
mark stopping
-> cancel/abort the live Provider worker
-> stop microphone and system-audio capture
-> encrypt and register every remaining local buffer
-> end the realtime session locally
-> queue the retryable finalization job
```

The background job loads pending chunks from the durable spool, not from transient UI or runtime memory. Authentication, network, quota, 429, 5xx, response-body delay, or Provider timeout errors may fail that background job, but they must not turn the local Stop operation into a realtime-stop timeout. After the user corrects Provider configuration, retry uses the current credential/configuration and resumes from the same encrypted chunks.

On restart, a spool session without an existing finalization job is recovered into exactly one retryable finalization job. Completed projects remove the spool. Corrupt or escaped paths fail closed.

## 11. Raw audio policy

- Real-time mode: never retain plaintext audio beyond the shortest local conversion window. Every finalized segment and Stop-time remainder is encrypted before remote processing. Delete the plaintext WAV immediately after encryption. Delete the encrypted chunk and encrypted transcript cache only after the required transcript and derived segment outputs are durably committed.
- Uploaded media: copy into managed local storage and encrypt.
