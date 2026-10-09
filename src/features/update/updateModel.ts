import { downloadProgress } from "../../lib/format";
import type { UpdateEvent, UpdateStatus } from "../../lib/types";

/** Where the update flow is. Pure data so every transition can be unit-tested. */
export type UpdatePhase = "idle" | "checking" | "notConfigured" | "upToDate" | "available" | "downloading" | "installing" | "error";

export interface UpdateModel {
  phase: UpdatePhase;
  /** Version that is running. */
  current: string | null;
  /** Version on offer. */
  version: string | null;
  notes: string | null;
  date: string | null;
  downloaded: number;
  total: number | null;
  error: string | null;
  checkedAt: number | null;
  /** The "replace the running app?" confirmation is showing. */
  confirming: boolean;
}

export const INITIAL_UPDATE: UpdateModel = {
  phase: "idle",
  current: null,
  version: null,
  notes: null,
  date: null,
  downloaded: 0,
  total: null,
  error: null,
  checkedAt: null,
  confirming: false,
};

export function isBusy(phase: UpdatePhase): boolean {
  return phase === "checking" || phase === "downloading" || phase === "installing";
}

/** A new check is only started when nothing else is running. */
export function canStartCheck(m: UpdateModel): boolean {
  return !isBusy(m.phase);
}

/** Installing needs a version the user was shown, and nothing else running. */
export function canStartInstall(m: UpdateModel): boolean {
  return m.phase === "available" && m.version !== null;
}

export function startChecking(m: UpdateModel): UpdateModel {
  return { ...m, phase: "checking", error: null, confirming: false };
}

export function applyStatus(m: UpdateModel, s: UpdateStatus, now: number): UpdateModel {
  switch (s.status) {
    case "notConfigured":
      return { ...m, phase: "notConfigured", error: null, checkedAt: now };
    case "upToDate":
      return { ...m, phase: "upToDate", current: s.current, version: null, notes: null, date: null, error: null, checkedAt: now };
    case "available":
      return { ...m, phase: "available", current: s.current, version: s.version, notes: s.notes, date: s.date, error: null, checkedAt: now };
  }
}

export function applyFailure(m: UpdateModel, message: string): UpdateModel {
  return { ...m, phase: "error", error: message, confirming: false };
}

export function askToInstall(m: UpdateModel): UpdateModel {
  return canStartInstall(m) ? { ...m, confirming: true } : m;
}

export function cancelConfirm(m: UpdateModel): UpdateModel {
  return { ...m, confirming: false };
}

export function startInstall(m: UpdateModel): UpdateModel {
  return { ...m, phase: "downloading", downloaded: 0, total: null, error: null, confirming: false };
}

export function applyEvent(m: UpdateModel, e: UpdateEvent): UpdateModel {
  if (e.kind === "downloading") return m.phase === "installing" ? m : { ...m, phase: "downloading", downloaded: e.downloaded, total: e.total };
  return { ...m, phase: "installing" };
}

/** 0..1 while the size is known, otherwise `null` (an indeterminate bar). */
export function progressOf(m: UpdateModel): number | null {
  return m.phase === "downloading" ? downloadProgress(m.downloaded, m.total) : null;
}

/** An update was found by the silent startup check: say so once, keep the model as it is. */
export function shouldAnnounce(prev: UpdateModel, next: UpdateModel): boolean {
  return next.phase === "available" && !(prev.phase === "available" && prev.version === next.version);
}
