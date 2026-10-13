import { describe, expect, it } from "vitest";

import { applyShell, collectShell } from "./backup";

describe("backup: the shell's part", () => {
  it("collects only the known preference keys", () => {
    const store: Record<string, string> = { "pb:site.prefs": '{"a.com":{"zoom":1.25}}', "pb:ai.history": "[1]", "pb:focus.config": "{bad" };
    expect(collectShell((k) => store[k] ?? null)).toEqual({ "site.prefs": { "a.com": { zoom: 1.25 } } });
  });
  it("restores only the known keys", () => {
    const written: Record<string, string> = {};
    const n = applyShell({ "site.prefs": { x: 1 }, "ai.history": [1] }, (k, v) => (written[k] = v));
    expect(n).toBe(1);
    expect(written).toEqual({ "pb:site.prefs": '{"x":1}' });
    expect(applyShell(null, () => {})).toBe(0);
  });
});
