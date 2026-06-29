export type SourceMediaKind = "audio" | "video" | "transcript" | "subtitle";

export interface SourceMediaAssetLike {
  id: string;
  kind: SourceMediaKind;
  originalFileName: string;
}

export interface TimelineSegmentLike {
  id: string;
  sourceId: string;
}

export function buildSourceMediaById(
  mediaAssets: readonly SourceMediaAssetLike[],
): ReadonlyMap<string, SourceMediaAssetLike> {
  return new Map(mediaAssets.map((asset) => [asset.id, asset]));
}

export function isUntimedTranscriptSegment(
  segment: TimelineSegmentLike,
  sourceMediaById: ReadonlyMap<string, SourceMediaAssetLike>,
): boolean {
  return sourceMediaById.get(segment.sourceId)?.kind === "transcript";
}

export function buildUntimedParagraphNumbers(
  timeline: readonly TimelineSegmentLike[],
  sourceMediaById: ReadonlyMap<string, SourceMediaAssetLike>,
): ReadonlyMap<string, number> {
  const countBySource = new Map<string, number>();
  const numberBySegment = new Map<string, number>();

  for (const segment of timeline) {
    if (!isUntimedTranscriptSegment(segment, sourceMediaById)) continue;
    const next = (countBySource.get(segment.sourceId) ?? 0) + 1;
    countBySource.set(segment.sourceId, next);
    numberBySegment.set(segment.id, next);
  }

  return numberBySegment;
}
