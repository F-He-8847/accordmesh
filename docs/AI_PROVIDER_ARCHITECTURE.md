# AccordMesh AI Provider Architecture

## 1. Provider-neutral rule

AccordMesh business logic must not depend on OpenAI-specific request or response types.

The official first implementation includes:

- `MockProvider`;
- `OpenAIProvider`.

Future community providers may include other cloud APIs, local transcription models, local language models, or OpenAI-compatible endpoints.

## 2. Capability-level interfaces

Use small interfaces rather than one monolithic provider:

```rust
trait FileTranscriptionProvider { ... }
trait RealtimeTranscriptionProvider { ... }
trait TranslationProvider { ... }
trait SegmentUnderstandingProvider { ... }
trait MeetingSynthesisProvider { ... }
trait ComparisonReportProvider { ... }
trait MeetingMinutesProvider { ... }
```

A provider may implement only a subset.

## 3. Capability declaration

```ts
interface ProviderCapabilities {
  fileTranscription: boolean;
  realtimeTranscription: boolean;
  textTranslation: boolean;
  segmentUnderstanding: boolean;
  meetingSynthesis: boolean;
  comparisonReport: boolean;
  meetingMinutes: boolean;
  supportsStreaming: boolean;
  supportsStructuredOutput: boolean;
  supportsLanguageAutoDetection: boolean;
  supportsCodeSwitching?: boolean;
  supportedInputFormats?: string[];
  supportedSourceLanguages?: string[];
  supportedTargetLanguages?: string[];
}
```

The UI must disable or explain unavailable actions before execution.

## 4. Provider definitions and credentials

```ts
interface ProviderDefinition {
  id: string;
  displayNameKey: string;
  credentialSchema: ProviderCredentialField[];
  configurationSchema: ProviderConfigField[];
  capabilities: ProviderCapabilities;
}
```

Do not assume every provider uses one API key. Support provider-defined fields such as endpoint, region, project ID, base URL, or local model path.

## 5. Unified domain output

Provider-specific responses must be converted into unified contracts before storage or UI use.

```text
Provider API -> Provider Adapter -> Domain Contract -> Storage/UI/Export
```

## 6. Prompt and schema separation

Prompts and JSON schemas live under `packages/prompts` and `packages/schemas`. Provider request code consumes them but does not redefine meeting rules.

## 7. MockProvider

MockProvider is the default development and demo provider. It must support deterministic fixtures for:

- streaming transcript;
- final transcript;
- translation;
- segment understanding;
- meeting synthesis;
- communication review;
- intelligent comparison;
- meeting minutes;
- timeout, quota, authentication, and unsupported-capability states.

## 8. OpenAIProvider

OpenAIProvider must be isolated in `providers/openai/`.

Requirements:

- credentials obtained only through vault service;
- model IDs configurable, not scattered constants;
- current API details kept behind adapter code;
- `store: false` or equivalent privacy option used where applicable and supported;
- errors mapped to stable application error codes;
- structured outputs validated against unified schemas;
- no assumption that every language or model has equal quality.

Do not make real API access mandatory for launching or demonstrating the app.

## 9. Community provider guide hooks

Provide a clear extension guide explaining:

- interfaces to implement;
- capability declaration;
- configuration UI schema;
- credential handling;
- output mapping;
- unsupported capability handling;
- privacy and pricing disclosure requirements.
