import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import vm from "node:vm";
import { describe, expect, it } from "vitest";

import { matchSections, resolveSection, SECTION_META } from "../pages/settingsIndex";
import type { LoginSummary, Tab } from "./types";
import { canOfferFill, fillMessage, filterLogins, usedText } from "./vault";

const login = (id: number, origin: string, username: string): LoginSummary => ({ id, origin, displayUrl: origin + "/", username, source: "password export", createdAt: 1, lastUsed: null });
const tab = (url: string, over: Partial<Tab> = {}): Tab => ({ id: 1, url, error: null, ...over }) as Tab;

describe("saved login list", () => {
  const list = [login(1, "https://github.com", "hamza"), login(2, "https://store.steampowered.com", "gamer42"), login(3, "https://github.com", "work-account")];

  it("filters by site and user name, every word must match", () => {
    expect(filterLogins(list, "").map((l) => l.id)).toEqual([1, 2, 3]);
    expect(filterLogins(list, "github").map((l) => l.id)).toEqual([1, 3]);
    expect(filterLogins(list, "github work").map((l) => l.id)).toEqual([3]);
    expect(filterLogins(list, "STEAM").map((l) => l.id)).toEqual([2]);
    expect(filterLogins(list, "nothing")).toEqual([]);
  });

  it("describes when a login was last used", () => {
    expect(usedText(null)).toBe("never");
    expect(usedText(Date.UTC(2026, 0, 2, 12))).toMatch(/2026|26/);
  });
});

describe("where filling is offered", () => {
  it("only on https pages and on this computer", () => {
    expect(canOfferFill(tab("https://example.com/login"))).toBe(true);
    expect(canOfferFill(tab("http://localhost:3000/"))).toBe(true);
    expect(canOfferFill(tab("http://127.0.0.1:8080/x"))).toBe(true);
    expect(canOfferFill(tab("http://example.com/login"))).toBe(false);
    expect(canOfferFill(tab("http://localhost.evil.example/"))).toBe(false);
    expect(canOfferFill(tab("browser://settings"))).toBe(false);
    expect(canOfferFill(tab("file:///c:/x.html"))).toBe(false);
    expect(canOfferFill(null)).toBe(false);
  });

  it("not on an error page", () => {
    expect(canOfferFill(tab("https://example.com/", { error: { url: "https://example.com/", code: "x", description: "y" } }))).toBe(false);
  });

  it("says what happened after every kind of fill outcome", () => {
    expect(fillMessage({ state: "filled", fields: 2 })).toContain("Filled 2 fields");
    expect(fillMessage({ state: "filled", fields: 1 })).toContain("Filled 1 field.");
    expect(fillMessage({ state: "usernameOnly", fields: 1 })).toContain("user name");
    expect(fillMessage({ state: "originMismatch", fields: 0 })).toContain("nothing was filled");
    expect(fillMessage({ state: "noForm", fields: 0 })).toContain("No form");
    // never claims success when nothing was filled
    expect(fillMessage({ state: "noForm", fields: 0 })).not.toMatch(/^Filled/);
  });
});

describe("settings search", () => {
  it("finds a section by its name or by what it contains", () => {
    expect(matchSections("").length).toBe(SECTION_META.length);
    expect(matchSections("password").map((s) => s.id)).toEqual(expect.arrayContaining(["passwords", "import"]));
    expect(matchSections("nvidia key").map((s) => s.id)).toEqual(["ai"]);
    expect(matchSections("zzzz")).toEqual([]);
    expect(matchSections("GAMING").map((s) => s.id)).toContain("gaming");
  });

  it("falls back to General for a remembered section that no longer exists", () => {
    expect(resolveSection("ai")).toBe("ai");
    expect(resolveSection("passwords")).toBe("passwords");
    expect(resolveSection("removed-section")).toBe("general");
    expect(resolveSection(null)).toBe("general");
  });

  it("has the AI and password sections that the rest of the app links to", () => {
    const ids = SECTION_META.map((s) => s.id);
    expect(ids).toEqual(expect.arrayContaining(["ai", "passwords", "import", "about"]));
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("the scripts that fill a page", () => {
  const read = (f: string) => readFileSync(join(process.cwd(), "src-tauri/src/native", f), "utf8");

  it("are valid JavaScript function expressions that take one argument", () => {
    for (const file of ["fill_login.js", "fill_profile.js"]) {
      const fn = vm.runInNewContext(`(${read(file)})`);
      expect(typeof fn, file).toBe("function");
      expect((fn as (a: unknown) => unknown).length).toBe(1);
    }
  });

  it("do nothing at all on a page whose origin is not the one they were built for", () => {
    for (const file of ["fill_login.js", "fill_profile.js"]) {
      const fn = vm.runInNewContext(`(${read(file)})`, { location: { origin: "https://evil.example" } }) as (a: unknown) => string;
      // Any DOM access would throw here: the sandbox has no document.
      expect(fn({ origin: "https://example.com", username: "u", password: "p", fields: { "given-name": "x" } })).toBe("origin-mismatch");
    }
  });
});

describe("repository hygiene", () => {
  it("no export of passwords or API keys is tracked, and fixtures hold only public model names", () => {
    const tracked = execSync("git ls-files", { cwd: process.cwd(), encoding: "utf8" }).split("\n").filter(Boolean);
    expect(tracked.filter((f) => /password.*\.csv$/i.test(f))).toEqual([]);
    const fixture = readFileSync(join(process.cwd(), "src-tauri/tests/fixtures/nvidia-models-2026-10-10.json"), "utf8");
    expect(fixture).not.toMatch(/nvapi-[A-Za-z0-9_-]{10,}/);
  });
});
