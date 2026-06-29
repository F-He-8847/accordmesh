import type { MeetingProject, ProcessingJob, ProjectStatus } from "../../shared/types";

const ACTIVE_JOB_STATUSES = new Set(["queued", "running", "resumable", "cancelling"]);

export type DeleteGuardKey =
  | "library.deleteActiveGuard"
  | "library.deleteProcessingGuard";

export function deleteGuardKey(
  project: MeetingProject,
  jobs: ProcessingJob[] = [],
): DeleteGuardKey | null {
  if (project.status === "active") return "library.deleteActiveGuard";
  if (
    project.status === "processing" ||
    jobs.some((job) => ACTIVE_JOB_STATUSES.has(job.status))
  ) {
    return "library.deleteProcessingGuard";
  }
  return null;
}

export function canDeleteProject(
  project: MeetingProject,
  jobs: ProcessingJob[] = [],
): boolean {
  return deleteGuardKey(project, jobs) === null;
}

export function statusDeleteGuardCode(
  status: ProjectStatus,
): "ERR_ACTIVE_SESSION" | "ERR_ACTIVE_JOB" | null {
  if (status === "active") return "ERR_ACTIVE_SESSION";
  if (status === "processing") return "ERR_ACTIVE_JOB";
  return null;
}
