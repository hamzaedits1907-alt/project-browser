import { describe, expect, it } from "vitest";

import { isFresh, pushSample } from "./perf";
import type { PerfSample } from "./types";

describe("perf history", () => {
  it("is bounded and drops the oldest", () => {
    let h: number[] = [];
    for (let i = 0; i < 100; i++) h = pushSample(h, i, 60);
    expect(h.length).toBe(60);
    expect(h[0]).toBe(40);
    expect(h.at(-1)).toBe(99);
  });
  it("does not mutate its input", () => {
    const h = [1, 2];
    pushSample(h, 3, 5);
    expect(h).toEqual([1, 2]);
  });
  it("treats old cached samples as stale", () => {
    const s = { timestamp: 1_000_000 } as PerfSample;
    expect(isFresh(s, 1_005_000)).toBe(true);
    expect(isFresh(s, 1_100_000)).toBe(false);
  });
});
