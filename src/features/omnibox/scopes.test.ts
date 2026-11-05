import { describe, expect, it } from "vitest";

import { inScope, parseScope, scopeHints } from "./scopes";

describe("address-bar scopes", () => {
  it("reads a scope and its words", () => {
    expect(parseScope("@tabs news site")).toEqual({ kind: "tab", key: "@tabs", query: "news site" });
    expect(parseScope("@History  rust")).toEqual({ kind: "history", key: "@history", query: "rust" });
    expect(parseScope("@unknown x")).toBeNull();
    expect(parseScope("@tabs")).toBeNull();
  });
  it("offers the scopes while one is being typed", () => {
    expect(scopeHints("@b").map((s) => s.url)).toEqual(["@bookmarks "]);
    expect(scopeHints("@").length).toBe(3);
    expect(scopeHints("@tabs x")).toEqual([]);
    expect(scopeHints("hello")).toEqual([]);
  });
  it("keeps only the scope's kind", () => {
    const list = [
      { kind: "tab" as const, title: "a", url: "u", tabId: 1 },
      { kind: "history" as const, title: "b", url: "v", tabId: null },
    ];
    expect(inScope(list, "tab").map((s) => s.title)).toEqual(["a"]);
  });
});
