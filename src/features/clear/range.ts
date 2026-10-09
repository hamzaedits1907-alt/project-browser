export type Range = "hour" | "day" | "week" | "month" | "all";

const RANGE_MS: Record<Exclude<Range, "all">, number> = {
  hour: 3_600_000,
  day: 86_400_000,
  week: 7 * 86_400_000,
  month: 28 * 86_400_000,
};

/** Start of the history range to clear; `null` means everything. */
export function sinceFor(range: Range, now = Date.now()): number | null {
  return range === "all" ? null : now - RANGE_MS[range];
}
