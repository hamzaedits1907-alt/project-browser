import { beforeEach, describe, expect, it, vi } from "vitest";

const overlayOpen = vi.fn<() => Promise<string | null>>();
const overlayClose = vi.fn<(focus?: boolean) => Promise<void>>();

vi.mock("./ipc", () => ({
  api: { overlayOpen: () => overlayOpen(), overlayClose: (f?: boolean) => overlayClose(f) },
}));

let activeUrl: string | null = "https://example.com/";
vi.mock("./store", () => ({
  getState: () => ({}),
  activeTab: () => (activeUrl ? { url: activeUrl } : null),
}));

const { acquireOverlay } = await import("./overlay");

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("page overlay", () => {
  beforeEach(() => {
    overlayOpen.mockReset().mockResolvedValue("data:image/jpeg;base64,AA");
    overlayClose.mockReset().mockResolvedValue(undefined);
    activeUrl = "https://example.com/";
  });

  it("hides the page once for several popups and restores it after the last one closes", async () => {
    const a = acquireOverlay();
    const b = acquireOverlay();
    await Promise.all([a.ready, b.ready]);
    expect(overlayOpen).toHaveBeenCalledTimes(1);
    a.release();
    await flush();
    expect(overlayClose).not.toHaveBeenCalled();
    b.release();
    await flush();
    expect(overlayClose).toHaveBeenCalledTimes(1);
  });

  it("does not hide anything for browser:// pages", async () => {
    activeUrl = "browser://settings";
    const h = acquireOverlay();
    await h.ready;
    expect(overlayOpen).not.toHaveBeenCalled();
    h.release();
    await flush();
    expect(overlayClose).not.toHaveBeenCalled();
  });

  it("a popup closed before the page was hidden never hides it", async () => {
    const h = acquireOverlay();
    h.release();
    await h.ready;
    await flush();
    expect(overlayOpen).not.toHaveBeenCalled();
  });

  it("release is idempotent", async () => {
    const h = acquireOverlay();
    await h.ready;
    h.release();
    h.release();
    await flush();
    expect(overlayClose).toHaveBeenCalledTimes(1);
  });

  it("survives a failed snapshot", async () => {
    overlayOpen.mockRejectedValue(new Error("no snapshot"));
    const h = acquireOverlay();
    await h.ready;
    h.release();
    await flush();
    expect(overlayClose).toHaveBeenCalledTimes(1);
  });
});
