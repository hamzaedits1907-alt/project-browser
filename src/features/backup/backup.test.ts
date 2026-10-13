import { describe, expect, it } from "vitest";

import { applyShell, collectShell } from "./backup";

describe("backup: the shell's part", () => {
  it("collects only the known preference keys", () => {
    const store: Record<string, string> = { "pb:site.prefs": '{"a.com":{"zoom":1.25}}', "pb:ai.history": "[1]", "pb:focus.config": "{bad" };
    expect(collectShell((k) => store[k] ?? null)).toEqual({ "site.prefs": { "a.com": { zoom: 1.25 } } });
  });
  it("includes notes, one key per site plus the scratchpad", () => {
    const store: Record<string, string> = { "pb:notes.site.a.com": '"hello"', "pb:notes.all": '"pad"' };
    expect(collectShell((k) => store[k] ?? null, Object.keys(store))).toEqual({ "notes.site.a.com": "hello", "notes.all": "pad" });
    const written: Record<string, string> = {};
    expect(applyShell({ "notes.site.a.com": "hello" }, (k, v) => (written[k] = v))).toBe(1);
    expect(written).toEqual({ "pb:notes.site.a.com": '"hello"' });
  });

  it("restores only the known keys", () => {
    const written: Record<string, string> = {};
    const n = applyShell({ "site.prefs": { x: 1 }, "ai.history": [1] }, (k, v) => (written[k] = v));
    expect(n).toBe(1);
    expect(written).toEqual({ "pb:site.prefs": '{"x":1}' });
    expect(applyShell(null, () => {})).toBe(0);
  });
});
