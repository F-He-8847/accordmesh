import type { MeetingProject, SelectedFile } from "../../shared/types";

export function isRealtimeProject(project: MeetingProject) {
  return project.origin === "realtime_online" || project.origin === "realtime_in_person";
}

export function isAttachableRealtimeProject(project: MeetingProject) {
  return (
    isRealtimeProject(project) &&
    project.status === "completed" &&
    project.mediaAssetIds.length === 0 &&
    !project.hasComparison
  );
}

export function isRecordingSelection(file: SelectedFile | null) {
  return file?.kind === "audio" || file?.kind === "video";
}
