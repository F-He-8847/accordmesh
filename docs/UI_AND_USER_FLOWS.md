# AccordMesh UI and User Flows

## 1. Main navigation

First official version pages:

- Unlock / first-run setup;
- Meeting Library;
- Start Online Meeting;
- Start In-Person Meeting;
- Upload Meeting Material;
- Meeting Project Detail;
- Provider Settings;
- Language Settings;
- General Settings;
- Real-Time Overlay.

## 2. First-run flow

1. Welcome and local-first explanation.
2. Create local password.
3. Confirm no password recovery.
4. Create encrypted vault.
5. Configure a provider or choose MockProvider.
6. Select default output/translation/minutes languages.
7. Open Meeting Library.

## 3. Unlock flow

Display one primary field:

> Enter your local password to unlock

Provide reset-vault access behind a clear destructive warning.

## 4. Library

Show project cards/rows with:

- title;
- date/time;
- origin: online, in-person, upload;
- processing state;
- whether uploaded media is attached;
- whether comparison/minutes exist.

Actions:

- open;
- rename;
- delete.

Filters:

- origin;
- status;
- date sorting.

Search: title only in first version.

## 5. New online meeting

Before start:

- choose system audio device/source where applicable;
- choose microphone;
- show permission/device status;
- select source-language mode;
- choose translation and analysis language;
- choose Provider configuration;
- show privacy notice;
- create project and start.

## 6. New in-person meeting

Before start:

- choose microphone;
- perform sound check;
- select language settings;
- show privacy notice;
- create project and start.

Use neutral wording throughout.

## 7. Upload flow

- choose exactly one supported audio, video, transcript, or subtitle file;
- either create a new upload-only project or attach one recording to an eligible completed real-time project;
- show import/processing stages;
- allow cancellation;
- preserve completed stages on retry.

## 8. Project detail

Recommended sections/tabs:

- Overview;
- Timeline;
- Source Transcript;
- Translations;
- Core Understanding;
- Communication Review;
- Intelligent Comparison;
- Meeting Minutes;
- Attached Materials;
- Generation History.

Generated content is read-only.

## 9. Rename only

The only direct content edit is project title. Do not add editors for transcript, reports, or minutes.

## 10. Delete

Show exactly what will be deleted and state that it cannot be recovered.

## 11. Provider status

Before an action, detect required capability. If unsupported, show a localized capability explanation and do not launch a failing job.

## 12. Error UX

Use stable error-code mapping. Never display raw provider stack traces or secrets. Provide actionable states such as:

- permission required;
- audio device unavailable;
- invalid credentials;
- quota unavailable;
- network interruption;
- unsupported capability;
- processing can be resumed.
