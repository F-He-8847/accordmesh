import type { AnalysisArtifact } from "../../shared/types";

export type ArtifactsByType = Record<string, AnalysisArtifact[]>;
export type SelectedVersionByScope = Record<string, string>;

const segmentScopedArtifactTypes = new Set([
  "literal_translation",
  "segment_understanding",
]);

export function groupArtifactsByType(artifacts: AnalysisArtifact[]): ArtifactsByType {
  return artifacts.reduce<ArtifactsByType>((all, item) => {
    all[item.artifactType] = [...(all[item.artifactType] ?? []), item];
    return all;
  }, {});
}

export function artifactSelectionKey(artifact: AnalysisArtifact): string {
  if (!segmentScopedArtifactTypes.has(artifact.artifactType)) {
    return artifact.artifactType;
  }
  return `${artifact.artifactType}::${artifact.sourceIds.join("|")}`;
}

export function groupArtifactsBySelectionScope(
  artifacts: AnalysisArtifact[],
): AnalysisArtifact[][] {
  const groups = new Map<string, AnalysisArtifact[]>();
  for (const artifact of artifacts) {
    const key = artifactSelectionKey(artifact);
    groups.set(key, [...(groups.get(key) ?? []), artifact]);
  }
  return [...groups.values()];
}

function usableArtifacts(artifacts: AnalysisArtifact[]): AnalysisArtifact[] {
  return artifacts.filter(
    (artifact) => artifact.status === "completed" && artifact.payload !== null && artifact.payload !== undefined,
  );
}

function latestArtifact(artifacts: AnalysisArtifact[]): AnalysisArtifact | undefined {
  return usableArtifacts(artifacts).sort(
    (left, right) =>
      left.createdAt.localeCompare(right.createdAt) || left.id.localeCompare(right.id),
  ).at(-1);
}

export function selectedArtifactIdsForExport(
  byType: ArtifactsByType,
  selectedVersionByScope: SelectedVersionByScope,
): string[] {
  return Object.values(byType).flatMap((artifacts) =>
    groupArtifactsBySelectionScope(artifacts).flatMap((scopeArtifacts) => {
      const first = scopeArtifacts[0];
      if (!first) return [];
      const selectedId = selectedVersionByScope[artifactSelectionKey(first)];
      const selected = usableArtifacts(scopeArtifacts).find((artifact) => artifact.id === selectedId);
      const artifact = selected ?? latestArtifact(scopeArtifacts);
      return artifact ? [artifact.id] : [];
    }),
  );
}
