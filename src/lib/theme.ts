// Applies the user's look (theme, accent, shape, density, motion) to <html>. Everything visual reads CSS variables
// and data attributes set here, so a theme change is one style recalculation, not a re-render.

import { effectiveAccent, motionMode, themeDef } from "./settings";
import { readLocal, writeLocal } from "./storage";
import type { Settings } from "./types";

export interface AppliedLook {
  vars: Record<string, string>;
  attrs: Record<string, string>;
}

/** The CSS variables and data attributes for a set of preferences (pure, tested). */
export function lookFor(s: Settings): AppliedLook {
  const theme = themeDef(s.theme);
  const p = theme.palette;
  const [h, sat, light] = effectiveAccent(s);
  const custom = s.theme === "custom" || s.accent === "custom";
  const accent2 = custom || (s.accent !== "auto" && s.accent !== theme.id) ? (h + 70) % 360 : p.accent2;
  const vars: Record<string, string> = {
    "--ah": String(h),
    "--as": `${sat}%`,
    "--al": `${light}%`,
    "--ah2": String(accent2),
    "--glow": String(p.glow),
  };
  p.bg.forEach((c, i) => (vars[`--bg-${i}`] = c));
  p.text.forEach((c, i) => (vars[`--text-${i + 1}`] = c));
  const attrs: Record<string, string> = {
    theme: theme.id,
    effects: p.effects ? "on" : "off",
    corners: s.cornerStyle,
    density: s.density,
    motion: motionMode(s),
    tabs: s.tabStyle,
    gaming: String(s.gamingMode),
    vertical: String(s.verticalTabs),
    scheme: p.scheme ?? "dark",
  };
  return { vars, attrs };
}

let lastKey = "";

/** Applies the look to the document. Cross-fades when the colours change and motion is on. */
export function applyLook(s: Settings, root: HTMLElement = document.documentElement): void {
  const look = lookFor(s);
  const key = JSON.stringify(look);
  if (key === lastKey) return;
  const colorsChanged = lastKey !== "" && JSON.stringify(JSON.parse(lastKey).vars) !== JSON.stringify(look.vars);
  lastKey = key;
  const write = () => {
    for (const [k, v] of Object.entries(look.vars)) root.style.setProperty(k, v);
    for (const [k, v] of Object.entries(look.attrs)) root.dataset[k] = v;
  };
  const doc = root.ownerDocument as Document & { startViewTransition?: (cb: () => void) => unknown };
  if (colorsChanged && look.attrs.motion !== "off" && typeof doc.startViewTransition === "function") {
    doc.startViewTransition(write);
  } else {
    write();
  }
  // Remembered per viewer so the next start paints in the right colours before the core answers.
  writeLocal("look", look);
}

/** Paints the last look this viewer saw (before React and the core are ready). */
export function applyCachedLook(root: HTMLElement = document.documentElement): void {
  const look = readLocal<AppliedLook | null>("look", null);
  if (!look || typeof look !== "object" || !look.vars || !look.attrs) return;
  for (const [k, v] of Object.entries(look.vars)) if (k.startsWith("--") && typeof v === "string") root.style.setProperty(k, v);
  for (const [k, v] of Object.entries(look.attrs)) if (typeof v === "string") root.dataset[k] = v;
}
