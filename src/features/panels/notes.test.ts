import { describe, expect, it } from "vitest";

import { exportText, noteEntries, searchNotes } from "./notes";

const store = {
  "pb:notes.site.b.com": JSON.stringify("buy milk"),
  "pb:notes.site.a.com": JSON.stringify("Rust notes"),
  "pb:notes.all": JSON.stringify("scratch"),
  "pb:notes.site.empty.com": JSON.stringify("  "),
  "pb:other": JSON.stringify("x"),
};

describe("notes list", () => {
  it("lists the scratchpad first, then sites by name, leaving out empty notes and other keys", () => {
    expect(noteEntries(store).map((e) => e.site)).toEqual([null, "a.com", "b.com"]);
  });
  it("searches sites and text", () => {
    expect(searchNotes(noteEntries(store), "rust").map((e) => e.site)).toEqual(["a.com"]);
    expect(searchNotes(noteEntries(store), "B.COM").map((e) => e.site)).toEqual(["b.com"]);
    expect(searchNotes(noteEntries(store), "").length).toBe(3);
  });
  it("exports every note under its site", () => {
    expect(exportText(noteEntries(store))).toBe("# Scratchpad\n\nscratch\n\n# a.com\n\nRust notes\n\n# b.com\n\nbuy milk\n");
  });
});
