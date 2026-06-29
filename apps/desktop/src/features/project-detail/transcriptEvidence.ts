import type { MeetingProject, TimelineSegment, TrackRole } from "../../shared/types";
import type { SourceMediaAssetLike } from "./transcriptPresentation";

export type TranscriptEvidenceView = "both" | "realtime" | "recording";
export type TranscriptEvidenceKind = "realtime" | "recording" | "uploaded" | "unknown";

export interface TranscriptEvidencePartition {
  realtime: TimelineSegment[];
  recording: TimelineSegment[];
  uploaded: TimelineSegment[];
  unknown: TimelineSegment[];
  hasComparableSources: boolean;
}

export function classifyTranscriptEvidence(
  origin: MeetingProject["origin"],
  segment: Pick<TimelineSegment, "trackRole">,
): TranscriptEvidenceKind {
  if (segment.trackRole === "uploaded_media") {
    return origin === "upload_only" ? "uploaded" : "recording";
  }
  if (isRealtimeTrackRole(segment.trackRole)) return "realtime";
  return "unknown";
}

export function partitionTranscriptEvidence(
  origin: MeetingProject["origin"],
  timeline: readonly TimelineSegment[],
): TranscriptEvidencePartition {
  const partition: Omit<TranscriptEvidencePartition, "hasComparableSources"> = {
    realtime: [],
    recording: [],
    uploaded: [],
    unknown: [],
  };

  for (const segment of timeline) {
    partition[classifyTranscriptEvidence(origin, segment)].push(segment);
  }

  return {
    ...partition,
    hasComparableSources: partition.realtime.length > 0 && partition.recording.length > 0,
  };
}

export function mediaForSegment(
  segment: Pick<TimelineSegment, "sourceId">,
  sourceMediaById: ReadonlyMap<string, SourceMediaAssetLike>,
): SourceMediaAssetLike | undefined {
  return sourceMediaById.get(segment.sourceId);
}

function isRealtimeTrackRole(trackRole: TrackRole): boolean {
  return trackRole === "remote_system_audio" ||
    trackRole === "local_microphone" ||
    trackRole === "room_microphone";
}
