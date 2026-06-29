# AccordMesh Analysis Contracts

## 1. Separation of outputs

For each relevant segment, keep distinct:

- source transcript;
- literal text translation;
- concise core meaning;
- explicit intent;
- inferred intent;
- ambiguity;
- response guidance.

Do not merge these into one free-form paragraph.

## 2. LiteralTranslation

```ts
interface LiteralTranslation {
  id: string;
  segmentId: string;
  sourceLanguage?: string;
  targetLanguage: string;
  translatedText: string;
  providerId: string;
  modelId: string;
  promptVersion: string;
  createdAt: string;
}
```

Translation must preserve names, dates, numbers, amounts, negation, conditions, uncertainty, and commitment level as faithfully as possible.

## 3. SegmentUnderstanding

```ts
interface SegmentUnderstanding {
  segmentId: string;
  coreMeaning: string;
  explicitIntents: string[];
  inferredIntents: Array<{
    text: string;
    confidence: "low" | "medium" | "high";
  }>;
  keyFacts: StructuredFact[];
  ambiguities: string[];
  guidance: {
    answer: string[];
    explain: string[];
    ask: string[];
    confirm: string[];
  };
  evidenceRefs: EvidenceRef[];
}
```

Empty guidance categories remain empty and should be hidden in the UI.

## 4. MeetingContextSnapshot

```ts
interface MeetingContextSnapshot {
  topicSummary: string;
  confirmedFacts: string[];
  conditions: string[];
  constraints: string[];
  decisions: string[];
  openQuestions: string[];
  unresolvedIssues: string[];
  recentSegmentIds: string[];
}
```

## 5. PostMeetingAnalysis

Must contain:

- meeting purpose/overview;
- major topics;
- key facts;
- confirmed decisions;
- conditions and constraints;
- unresolved issues;
- ambiguities or disagreements;
- recommended follow-up actions;
- evidence references;
- uncertainty notes.

## 6. CommunicationReview

Rules:

- online dual-track projects may cautiously assess the user's responses;
- in-person single-microphone and upload-only projects must remain meeting-level unless user speech is reliably identifiable;
- cite timestamps and source text;
- avoid personality or psychological claims;
- distinguish missed answer, unclear answer, overlong explanation, missing confirmation, and unsupported commitment;
- provide improved wording only when evidence supports it.

## 7. IntelligentComparisonReport

Comparison is semantic, not line-by-line.

Unified comparison types:

- Fact;
- Intent;
- Decision;
- Condition;
- Constraint;
- UnresolvedIssue;
- Guidance;
- CommunicationGap.

Statuses:

- `confirmed`;
- `missed_in_realtime`;
- `incomplete_in_realtime`;
- `new_detail`;
- `refined`;
- `contradicted`;
- `uncertain`;
- `guidance_changed`;
- `conclusion_changed`.

```ts
interface IntelligentComparisonReport {
  overallAssessment: string;
  correctlyCaptured: ComparisonItem[];
  missedOrIncomplete: ComparisonItem[];
  correctedInterpretations: ComparisonItem[];
  newlyDiscovered: ComparisonItem[];
  guidanceRevisions: ComparisonItem[];
  conclusionChanges: ComparisonItem[];
  recommendedFollowUps: string[];
}
```

Every comparison item should reference real-time and/or uploaded-material evidence.

## 8. MeetingMinutesDraft

The UI may evolve, but stored meeting-minutes artifacts use this stable generic contract:

```ts
interface MeetingMinutesDraft {
  id: string;
  projectId: string;
  language: string;
  sourceArtifactIds: string[];
  sections: MeetingMinutesSection[];
  evidenceRefs: EvidenceRef[];
  schemaVersion: string;
  generatedAt: string;
}
```

Rules:

- one generic template only;
- do not invent named speakers, owners, or deadlines;
- generated minutes are read-only in the app;
- users edit after export.

## 9. Provenance

Every generated artifact records:

- source IDs;
- provider;
- model;
- Prompt version;
- Schema version;
- app version;
- creation time;
- generation-run ID;
- failure state if incomplete.
