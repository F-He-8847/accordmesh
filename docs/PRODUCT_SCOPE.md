# AccordMesh Product Scope

## 1. Product statement

AccordMesh is a free, open-source, local-first desktop assistant for multilingual and cross-cultural meeting understanding.

It helps users:

- understand the core meaning of current speech;
- obtain faithful text translation when needed;
- see what should be answered, explained, asked, or confirmed;
- preserve chronological meeting records;
- analyze uploaded meeting audio/video after the meeting;
- compare real-time assistance with the fuller uploaded recording semantically;
- generate a simple meeting-minutes draft;
- export structured output for downstream tools and workflows.

## 2. Official first-version scenarios

### 2.1 Online real-time meeting

Inputs:

- remote/system audio;
- local microphone.

Outputs:

- chronological source transcript;
- optional text translation;
- concise core meaning;
- response guidance;
- automatic local project record;
- later uploaded recording analysis;
- intelligent comparison report;
- meeting-minutes draft.

### 2.2 In-person real-time meeting

Input:

- selected room/laptop/USB microphone.

Outputs:

- chronological source transcript;
- optional text translation;
- neutral core meaning and next issues;
- automatic local project record;
- later uploaded recording analysis;
- intelligent comparison report;
- meeting-minutes draft.

Because a single room microphone does not reliably identify speakers, personalized blame or named-speaker attribution is forbidden.

### 2.3 Upload-only analysis

Inputs:

- common audio files;
- common video files;
- text transcript;
- subtitle files.

Outputs:

- source-language transcript when media is supplied;
- optional text translation;
- structured meeting synthesis;
- communication-gap analysis;
- meeting-minutes draft.

## 3. Official first-version language strategy

- Official UI: English only.
- Meeting transcription: multilingual, depending on selected provider capability.
- Translation: text only.
- Analysis output language: user-configurable.
- Meeting-minutes language: user-configurable.
- Community UI translations: supported through resource folders and validation.

## 4. User-controlled local library

Users can:

- search projects by title;
- sort/filter projects;
- open and view projects;
- rename the project title;
- attach related recording/video to a real-time project;
- regenerate analyses;
- export;
- delete.

Users cannot directly edit generated transcript, translation, analysis, comparison report, or minutes inside the first official version.

## 5. Explicit exclusions

The first official version does not include:

- a standalone user-facing audio/video recording library or playback feature;
- screen recording or video capture;
- named-speaker recognition;
- voiceprint enrollment;
- face or emotion analysis;
- lie detection;
- translated speech output;
- speech synthesis;
- automatic speaking or meeting-chat replies;
- cloud accounts or cloud synchronization;
- enterprise permissions or administration;
- multiple company-specific minutes templates;
- automatic external task/project-system integration;
- complex visual analysis of uploaded video.

## 6. Product quality principles

- Never replace the source transcript with translation or summary.
- Never present inference as fact.
- Never invent speaker identity, owner, deadline, or commitment.
- Preserve dates, amounts, numbers, negation, uncertainty, conditions, and commitments.
- Link important findings to timestamps and evidence.
- Prefer clear limitations over fabricated certainty.
