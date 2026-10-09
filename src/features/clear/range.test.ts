import { describe, expect, it } from "vitest";

import { sinceFor } from "./range";

describe("clear-data time range", () => {
  const now = 1_000_000_000_000;
  it("maps ranges to a start time and all-time to null", () => {
    expect(sinceFor("hour", now)).toBe(now - 3_600_000);
    expect(sinceFor("day", now)).toBe(now - 86_400_000);
    expect(sinceFor("month", now)).toBe(now - 28 * 86_400_000);
    expect(sinceFor("all", now)).toBeNull();
  });
});
