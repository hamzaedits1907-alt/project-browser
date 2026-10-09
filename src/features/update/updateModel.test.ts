import { describe, expect, it } from "vitest";

import {
  applyEvent,
  applyFailure,
  applyStatus,
  askToInstall,
  canStartCheck,
  canStartInstall,
  cancelConfirm,
  INITIAL_UPDATE,
  isBusy,
  progressOf,
  shouldAnnounce,
  startChecking,
  startInstall,
} from "./updateModel";

const AVAILABLE = { status: "available", current: "0.1.1", version: "0.1.2", notes: "Fixes", date: null } as const;

describe("update flow", () => {
  it("starts idle and may check", () => {
    expect(INITIAL_UPDATE.phase).toBe("idle");
    expect(canStartCheck(INITIAL_UPDATE)).toBe(true);
    expect(canStartInstall(INITIAL_UPDATE)).toBe(false);
  });

  it("shows 'checking' and refuses a second check while one is running", () => {
    const m = startChecking(INITIAL_UPDATE);
    expect(m.phase).toBe("checking");
    expect(isBusy(m.phase)).toBe(true);
    expect(canStartCheck(m)).toBe(false);
  });

  it("reports 'up to date' when no newer version exists", () => {
    const m = applyStatus(startChecking(INITIAL_UPDATE), { status: "upToDate", current: "0.1.1" }, 1000);
    expect(m).toMatchObject({ phase: "upToDate", current: "0.1.1", version: null, checkedAt: 1000 });
    expect(canStartInstall(m)).toBe(false);
  });

  it("offers an available version with its notes, and installing needs a confirmation first", () => {
    const m = applyStatus(startChecking(INITIAL_UPDATE), AVAILABLE, 5);
    expect(m).toMatchObject({ phase: "available", version: "0.1.2", notes: "Fixes" });
    expect(canStartInstall(m)).toBe(true);
    expect(m.confirming).toBe(false);
    const asked = askToInstall(m);
    expect(asked.confirming).toBe(true);
    expect(cancelConfirm(asked).confirming).toBe(false);
    expect(cancelConfirm(asked).phase).toBe("available");
  });

  it("cannot ask to install when nothing is on offer", () => {
    expect(askToInstall(INITIAL_UPDATE).confirming).toBe(false);
    expect(askToInstall(applyStatus(INITIAL_UPDATE, { status: "upToDate", current: "1" }, 1)).confirming).toBe(false);
  });

  it("tracks download progress, then installing, and blocks duplicate attempts meanwhile", () => {
    let m = startInstall(applyStatus(INITIAL_UPDATE, AVAILABLE, 1));
    expect(m.phase).toBe("downloading");
    expect(canStartCheck(m)).toBe(false);
    expect(canStartInstall(m)).toBe(false);
    m = applyEvent(m, { kind: "downloading", downloaded: 10, total: 40 });
    expect(progressOf(m)).toBe(0.25);
    m = applyEvent(m, { kind: "downloading", downloaded: 40, total: 40 });
    expect(progressOf(m)).toBe(1);
    m = applyEvent(m, { kind: "installing" });
    expect(m.phase).toBe("installing");
    expect(progressOf(m)).toBeNull();
    // A late progress event must not move an installing update back to downloading.
    expect(applyEvent(m, { kind: "downloading", downloaded: 1, total: 40 }).phase).toBe("installing");
  });

  it("shows an indeterminate bar when the size is unknown", () => {
    const m = applyEvent(startInstall(applyStatus(INITIAL_UPDATE, AVAILABLE, 1)), { kind: "downloading", downloaded: 123, total: null });
    expect(progressOf(m)).toBeNull();
  });

  it("keeps the failure message for the user and allows another try", () => {
    const m = applyFailure(startChecking(INITIAL_UPDATE), "Couldn't reach the update server.");
    expect(m).toMatchObject({ phase: "error", error: "Couldn't reach the update server." });
    expect(canStartCheck(m)).toBe(true);
    expect(applyStatus(m, { status: "upToDate", current: "0.1.1" }, 2).error).toBeNull();
  });

  it("announces a newly found version once, not again for the same one", () => {
    const found = applyStatus(INITIAL_UPDATE, AVAILABLE, 1);
    expect(shouldAnnounce(INITIAL_UPDATE, found)).toBe(true);
    expect(shouldAnnounce(found, found)).toBe(false);
    expect(shouldAnnounce(found, applyStatus(INITIAL_UPDATE, { ...AVAILABLE, version: "0.1.3" }, 2))).toBe(true);
    expect(shouldAnnounce(INITIAL_UPDATE, applyStatus(INITIAL_UPDATE, { status: "upToDate", current: "1" }, 1))).toBe(false);
  });

  it("reports builds without an update server", () => {
    expect(applyStatus(INITIAL_UPDATE, { status: "notConfigured" }, 3).phase).toBe("notConfigured");
  });
});
