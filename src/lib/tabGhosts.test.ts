import { describe, expect, it } from "vitest";

import { closedGhosts } from "./tabGhosts";

const d = (id: number, size = 200) => ({ id, title: `T${id}`, size });

describe("closing animation ghosts", () => {
  it("places a closed tab before the tab that followed it", () => {
    expect(closedGhosts([d(1), d(2), d(3)], new Set([1, 3]))).toEqual([{ ...d(2), before: 3 }]);
  });
  it("puts the last tab's ghost at the end and skips neighbours that closed too", () => {
    expect(closedGhosts([d(1), d(2), d(3)], new Set([1]))).toEqual([
      { ...d(2), before: null },
      { ...d(3), before: null },
    ]);
  });
  it("makes no ghost for tabs that are still there, were never drawn, or for mass closes beyond the cap", () => {
    expect(closedGhosts([d(1), d(2)], new Set([1, 2]))).toEqual([]);
    expect(closedGhosts([d(1, 0)], new Set())).toEqual([]);
    expect(closedGhosts(Array.from({ length: 20 }, (_, i) => d(i)), new Set()).length).toBe(8);
  });
});
