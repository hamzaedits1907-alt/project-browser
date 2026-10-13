import { describe, expect, it } from "vitest";

import { formatDuration, nextSample, secondsLeft } from "./downloadRate";

describe("download rate", () => {
  it("needs two samples half a second apart before it shows a speed", () => {
    const a = nextSample(undefined, 0, 0);
    expect(a.rate).toBeNull();
    expect(nextSample(a, 1000, 200)).toBe(a);
    const b = nextSample(a, 1_000_000, 1000);
    expect(b.rate).toBe(1_000_000);
  });
  it("smooths changes and restarts when the bytes go backwards (a retried download)", () => {
    const b = { bytes: 1_000_000, at: 1000, rate: 1_000_000 };
    const c = nextSample(b, 1_500_000, 2000);
    expect(c.rate).toBeCloseTo(850_000);
    expect(nextSample(c, 10, 3000).rate).toBeNull();
  });
  it("shows a time left only with a known size and a real speed", () => {
    expect(secondsLeft(0, null, 1000)).toBeNull();
    expect(secondsLeft(0, 10_000, null)).toBeNull();
    expect(secondsLeft(10_000, 10_000, 1000)).toBeNull();
    expect(secondsLeft(5_000, 10_000, 1000)).toBe(5);
    expect(formatDuration(5)).toBe("5 s left");
    expect(formatDuration(600)).toBe("10 min left");
    expect(formatDuration(3900)).toBe("1 h 5 min left");
  });
});
