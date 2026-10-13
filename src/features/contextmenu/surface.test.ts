import { describe, expect, it } from "vitest";

import { nextHot } from "./MenuSurface";

describe("right-click menu keyboard highlight", () => {
  it("starts at the first or last entry and wraps around", () => {
    expect(nextHot("ArrowDown", -1, 4)).toBe(0);
    expect(nextHot("ArrowUp", -1, 4)).toBe(3);
    expect(nextHot("ArrowDown", 3, 4)).toBe(0);
    expect(nextHot("ArrowUp", 0, 4)).toBe(3);
    expect(nextHot("Home", 2, 4)).toBe(0);
    expect(nextHot("End", 0, 4)).toBe(3);
    expect(nextHot("ArrowDown", -1, 0)).toBe(-1);
  });
});
