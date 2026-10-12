import type { GroupColor, TabGroup } from "./types";
import { GROUP_COLORS } from "./types";

/** Display colours for tab groups, tuned to stay readable on every dark theme. */
export const GROUP_HEX: Record<GroupColor, string> = {
  grey: "#9aa3b5",
  blue: "#5aa2ff",
  red: "#ff5b6e",
  yellow: "#ffcc4d",
  green: "#4ade80",
  pink: "#ff7ac6",
  purple: "#b48cff",
  cyan: "#3fd7ee",
  orange: "#ff9a4a",
};

export function groupHex(color: string): string {
  return GROUP_HEX[(GROUP_COLORS as readonly string[]).includes(color) ? (color as GroupColor) : "blue"];
}

/** The colour a new group gets: the first one no group in this window uses yet. */
export function nextGroupColor(groups: TabGroup[]): GroupColor {
  const used = new Set(groups.map((g) => g.color));
  return GROUP_COLORS.find((c) => !used.has(c) && c !== "grey") ?? "blue";
}

/** What a group chip says: its title, or its tab count when it has none. */
export function groupLabel(group: TabGroup, count: number): string {
  return group.title || `${count} ${count === 1 ? "tab" : "tabs"}`;
}
