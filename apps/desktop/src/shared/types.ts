export type ProjectOrigin = "realtime_online" | "realtime_in_person" | "upload_only";
export type ProjectStatus = "active" | "completed" | "processing" | "failed";
export type RealtimeMode = "online" | "in_person";
export type TrackRole = "remote_system_audio" | "local_microphone" | "room_microphone" | "uploaded_media" | "unknown";
export type MediaKind = "audio" | "video" | "transcript" | "subtitle";
export type ExportFormat = "markdown" | "txt" | "json";

export interface MeetingProject {
  id: string;
  title: string;
  origin: ProjectOrigin;
  status: ProjectStatus;
  createdAt: string;
  updatedAt: string;
  realtimeSessionId?: string;
  mediaAssetIds: string[];
  timelineSegmentIds: string[];
  artifactIds: string[];
  generationRunIds: string[];
  hasComparison?: boolean;
  hasMinutes?: boolean;
}

export interface TimelineSegment {
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
  warnings: string[];
  createdAt: string;
}

export interface MediaAsset {
  id: string;
  projectId: string;
  kind: MediaKind;
  originalFileName: string;
  importedAt: string;
  durationMs?: number;
  sha256: string;
  processingStatus: string;
}

export interface AnalysisArtifact {
  id: string;
  projectId: string;
  artifactType: string;
  sourceIds: string[];
  schemaVersion: string;
  promptVersion: string;
  providerId: string;
  modelId: string;
  appVersion: string;
  createdAt: string;
  status: string;
  payload: unknown;
}

export interface GenerationRun {
  id: string;
  projectId: string;
  artifactId?: string;
  providerId: string;
  modelId: string;
  promptVersion: string;
  schemaVersion: string;
  sourceIds: string[];
  status: string;
  errorCode?: string;
  createdAt: string;
  startedAt?: string;
  completedAt?: string;
}

export interface RealtimeSession { id: string; projectId: string; mode: RealtimeMode; startedAt: string; endedAt?: string; status: "starting" | "running" | "paused" | "completed" | "interrupted"; }
export interface RealtimeStateUpdate { projectId: string; status: "running" | "paused" | "completed" | "interrupted"; }
export interface ProcessingJob { id: string; projectId?: string; assetId?: string; kind: string; status: string; stage: string; progress: number; priority: number; retryCount: number; errorCode?: string; createdAt: string; startedAt?: string; updatedAt: string; completedAt?: string; }

export interface ProjectDetail {
  project: MeetingProject;
  timeline: TimelineSegment[];
  mediaAssets: MediaAsset[];
  artifacts: AnalysisArtifact[];
  generationRuns: GenerationRun[];
  realtimeSession?: RealtimeSession;
  jobs: ProcessingJob[];
}

export interface ProviderCapabilities {
  fileTranscription: boolean;
  realtimeTranscription: boolean;
  textTranslation: boolean;
  segmentUnderstanding: boolean;
  meetingSynthesis: boolean;
  communicationReview: boolean;
  comparisonReport: boolean;
  meetingMinutes: boolean;
  supportsStreaming: boolean;
  supportsStructuredOutput: boolean;
  supportsLanguageAutoDetection: boolean;
  supportsCodeSwitching: boolean;
  supportedInputFormats: string[];
  supportedSourceLanguages: string[];
  supportedTargetLanguages: string[];
}

export interface ProviderModelAssignment {
  capability: keyof ProviderCapabilities;
  configurationFieldId: string;
}

export interface ProviderDefinition {
  id: string;
  displayNameKey: string;
  credentialSchema: Array<{
    id: string;
    labelKey: string;
    fieldType: string;
    required: boolean;
    secret: boolean;
    defaultValue?: string;
  }>;
  configurationSchema: Array<{
    id: string;
    labelKey: string;
    fieldType: string;
    required: boolean;
    secret: boolean;
    defaultValue?: string;
  }>;
  modelAssignments: ProviderModelAssignment[];
  capabilities: ProviderCapabilities;
}

export interface SelectedFile { selectionToken: string; originalFileName: string; kind: MediaKind; size: number; mimeType?: string; }
export interface ProviderConfigurationStatus {
  providerId: string;
  stored: boolean;
  configured: boolean;
  configuredFields: string[];
  credentialFieldsConfigured: string[];
  missingRequiredFields: string[];
  configuration: Record<string, unknown>;
  maskedSummary: "ready" | "credential_missing" | "configuration_incomplete" | "configuration_invalid" | "not_configured";
  updatedAt?: string;
}
export interface AudioDeviceInfo { id: string; label: string; isDefault: boolean; permissionStatus: string; available: boolean; sampleRate?: number; channels?: number; }
export interface SoundCheck { level: number; peak: number; lowVolume: boolean; excessiveNoise: boolean; clipping: boolean; status: string; }
export interface SystemAudioStatus { available: boolean; supported: boolean; backend: string; permissionStatus: string; deviceLabel: string; requiresRestart: boolean; errorCode?: string; }

export interface ResetVaultStatus {
  activeRealtimeSessions: number;
  cleanupPendingSessions: number;
  activeJobs: number;
  operationsInFlight: number;
  resetInProgress: boolean;
  recoveryRequired: boolean;
  canStart: boolean;
  activeWorkBlocksReset: boolean;
}

export interface LanguagePreferences {
  uiLocale: "en";
  sourceLanguageMode: "auto" | "specified";
  sourceLanguage?: string;
  translationTargetLanguage: string;
  analysisOutputLanguage: string;
  minutesOutputLanguage: string;
}
