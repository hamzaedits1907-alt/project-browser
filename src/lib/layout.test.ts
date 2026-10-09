import { describe, expect, it } from "vitest";

import { sameLayout, snapRect } from "./layout";
import type { Viewport } from "./types";

const vp: Viewport = { width: 1280, height: 800, scale: 1.25, maximized: false, screenX: 0, screenY: 0 };

describe("content layout", () => {
  it("snaps edges to whole device pixels", () => {
    const r = snapRect({ x: 0.3, y: 118.4, width: 1279.5, height: 681.3 }, 1.25);
    for (const edge of [r.x, r.y, r.x + r.width, r.y + r.height]) {
      expect((edge * 1.25) % 1).toBeCloseTo(0, 5);
    }
    expect(r.width).toBeGreaterThan(1279);
    expect(r.height).toBeGreaterThan(681);
  });
  it("never reports negative sizes", () => {
    expect(snapRect({ x: 0, y: 0, width: -5, height: 0 }, 1).width).toBe(0);
  });
  it("detects identical layouts so unchanged geometry is not re-sent", () => {
    const a = { content: { x: 0, y: 118, width: 1280, height: 682 }, viewport: vp };
    expect(sameLayout(a, { content: { ...a.content }, viewport: { ...vp } })).toBe(true);
    expect(sameLayout(a, { content: { ...a.content, height: 600 }, viewport: vp })).toBe(false);
    expect(sameLayout(a, { content: a.content, viewport: { ...vp, maximized: true } })).toBe(false);
    expect(sameLayout(null, a)).toBe(false);
  });
});
