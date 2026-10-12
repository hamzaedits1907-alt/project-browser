import { describe, expect, it } from "vitest";

import { DEFAULT_SETTINGS, THEME_DEFS, THEMES, effectiveAccent, motionMode, moveItem, normalizeSiteUrl, toggleItem } from "./settings";
import { lookFor } from "./theme";

describe("themes", () => {
  it("every theme id has a definition with six surfaces and four text tones", () => {
    expect(THEME_DEFS.map((t) => t.id).sort()).toEqual([...THEMES].sort());
    for (const t of THEME_DEFS) {
      expect(t.palette.bg).toHaveLength(6);
      expect(t.palette.text).toHaveLength(4);
    }
  });
  it("uses the theme's accent unless one is chosen, and the custom hue for Custom", () => {
    expect(effectiveAccent({ theme: "violet", accent: "auto", customHue: 10 })).toEqual([266, 100, 71]);
    expect(effectiveAccent({ theme: "violet", accent: "lime", customHue: 10 })).toEqual([96, 78, 56]);
    expect(effectiveAccent({ theme: "neon", accent: "custom", customHue: 400 })[0]).toBe(40);
    expect(effectiveAccent({ theme: "custom", accent: "auto", customHue: 123 })[0]).toBe(123);
  });
  it("turns preferences into variables and attributes", () => {
    const look = lookFor({ ...DEFAULT_SETTINGS, theme: "oled", cornerStyle: "rounded", density: "compact", tabStyle: "pill", verticalTabs: true });
    expect(look.vars["--bg-0"]).toBe("#000000");
    expect(look.attrs).toMatchObject({ theme: "oled", corners: "rounded", density: "compact", tabs: "pill", vertical: "true", motion: "standard" });
    expect(lookFor({ ...DEFAULT_SETTINGS, theme: "minimal" }).attrs.effects).toBe("off");
  });
  it("switches motion off for reduce-motion, the Performance theme and Gaming Mode's setting", () => {
    expect(motionMode({ ...DEFAULT_SETTINGS, motion: "expressive" })).toBe("expressive");
    expect(motionMode({ ...DEFAULT_SETTINGS, reduceMotion: true })).toBe("off");
    expect(motionMode({ ...DEFAULT_SETTINGS, theme: "performance" })).toBe("off");
    expect(motionMode({ ...DEFAULT_SETTINGS, gamingMode: true, gamingReduceAnimations: true })).toBe("off");
    expect(motionMode({ ...DEFAULT_SETTINGS, gamingMode: true, gamingReduceAnimations: false })).toBe("standard");
  });
});

describe("customization lists", () => {
  it("moves and toggles items keeping catalogue order for re-added ones", () => {
    expect(moveItem(["a", "b", "c"], "b", -1)).toEqual(["b", "a", "c"]);
    expect(moveItem(["a", "b"], "a", -1)).toEqual(["a", "b"]);
    const catalog = ["a", "b", "c", "d"];
    expect(toggleItem(["a", "d"], "c", true, catalog)).toEqual(["a", "c", "d"]);
    expect(toggleItem(["a", "c"], "a", false, catalog)).toEqual(["c"]);
  });
  it("accepts only web addresses as pinned sites", () => {
    expect(normalizeSiteUrl("discord.com/app")).toBe("https://discord.com/app");
    expect(normalizeSiteUrl("javascript:alert(1)")).toBeNull();
    expect(normalizeSiteUrl("localhost")).toBeNull();
    expect(normalizeSiteUrl("  ")).toBeNull();
  });
});
