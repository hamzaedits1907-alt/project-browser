import { describe, expect, it } from "vitest";

import { ACCENTS, DEFAULT_SETTINGS, accentOrDefault, backgroundOrDefault, motionReduced, parseHostList, patch } from "./settings";

describe("settings helpers", () => {
  it("falls back to known accents and backgrounds", () => {
    expect(accentOrDefault("violet")).toBe("violet");
    expect(accentOrDefault("neon-pink")).toBe("auto");
    expect(accentOrDefault("ion")).toBe("auto");
    expect(backgroundOrDefault("aurora")).toBe("aurora");
    expect(backgroundOrDefault("x")).toBe("grid");
    expect(accentOrDefault(DEFAULT_SETTINGS.accent)).toBe("auto");
    expect(ACCENTS.length).toBeGreaterThan(5);
  });
  it("reduces motion by preference or by Gaming Mode", () => {
    expect(motionReduced(DEFAULT_SETTINGS)).toBe(false);
    expect(motionReduced(patch(DEFAULT_SETTINGS, { reduceMotion: true }))).toBe(true);
    expect(motionReduced(patch(DEFAULT_SETTINGS, { gamingMode: true }))).toBe(true);
    expect(motionReduced(patch(DEFAULT_SETTINGS, { gamingMode: true, gamingReduceAnimations: false }))).toBe(false);
  });
  it("patch does not mutate", () => {
    const next = patch(DEFAULT_SETTINGS, { accent: "lime" });
    expect(next.accent).toBe("lime");
    expect(DEFAULT_SETTINGS.accent).toBe("auto");
  });
  it("parses never-sleep host lists", () => {
    expect(parseHostList("https://YouTube.com/watch\n music.youtube.com, youtube.com ;")).toEqual(["music.youtube.com", "youtube.com"]);
    expect(parseHostList("   ")).toEqual([]);
  });
});
