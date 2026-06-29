import type { InactivityLockMinutes } from "./languagePreferences";

export type LockTrigger = "manual" | "inactivity" | "background";

export function shouldAutoLock(
  lastActivityAt: number,
  now: number,
  inactivityMinutes: InactivityLockMinutes,
): boolean {
  if (inactivityMinutes === null) return false;
  return now - lastActivityAt >= inactivityMinutes * 60_000;
}

export function lockErrorForTrigger(
  error: unknown,
  trigger: LockTrigger,
): string {
  const code = String(error);
  if (trigger !== "manual" && code === "ERR_ACTIVE_SESSION") {
    return "ERR_AUTO_LOCK_DEFERRED";
  }
  if (
    trigger !== "manual" &&
    (code === "ERR_LOCK_ACTIVE_JOB" || code === "ERR_LOCK_BUSY" || code === "ERR_REALTIME_CLEANUP_BUSY")
  ) {
    return "ERR_AUTO_LOCK_DEFERRED_BUSY";
  }
  return code;
}
