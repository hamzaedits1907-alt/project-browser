import { describe, expect, it } from "vitest";

import { parseClaimBattle, parseComparison } from "../research/provenance";
import { toSources } from "../research/sources";
import { MEMORY_BODY_LIMIT, draftFromAnswer, draftFromResearch, formatTags, parseTags } from "./memory";

const PAGE_A = "The card has 12 GB of GDDR6X memory and draws 200 W under typical load.";
const PAGE_B = "The vendor lists 16 GB of memory for this card in its datasheet.";
const sources = toSources([
  { tabId: 1, url: "https://a.example/review", title: "A review", text: PAGE_A, selection: "", totalChars: PAGE_A.length, truncated: false },
  { tabId: 2, url: "https://b.example/spec", title: "B spec", text: PAGE_B, selection: "", totalChars: PAGE_B.length, truncated: false },
]);

describe("keeping research in Project Memory", () => {
  const comparison = parseComparison(
    JSON.stringify({
      summary: "They disagree about memory.",
      claims: [{ text: "12 GB", sources: ["S1"], quote: "has 12 GB of GDDR6X memory" }, { text: "Fake", sources: ["S2"], quote: "made up sentence that is not there" }],
      missing: ["A teardown"],
    }),
    sources,
  );

  it("saves a comparison with its pages, its quotes and its unverified flags", () => {
    if (!comparison.ok) throw new Error(comparison.error);
    const draft = draftFromResearch(comparison.result, sources);
    expect(draft.kind).toBe("research");
    expect(draft.sources).toEqual([
      { url: "https://a.example/review", title: "A review" },
      { url: "https://b.example/spec", title: "B spec" },
    ]);
    expect(draft.body).toContain("- 12 GB [S1]");
    expect(draft.body).toContain("“has 12 GB of GDDR6X memory”");
    expect(draft.body).toContain("- Fake [S2 (quote not verified)]");
    expect(draft.body).not.toContain("made up sentence");
    expect(draft.body).toContain("S1: A review https://a.example/review");
    expect(draft.tags).toEqual(["comparison"]);
  });

  it("saves a claim check with its leaning described as a reading, never as proof", () => {
    const claim = parseClaimBattle(JSON.stringify({ supporting: [{ text: "12 GB", sources: ["S1"], quote: "has 12 GB of GDDR6X memory" }] }), "How much memory?", sources);
    if (!claim.ok) throw new Error(claim.error);
    const draft = draftFromResearch(claim.result, sources, "  Memory question ");
    expect(draft.title).toBe("Memory question");
    expect(draft.body).toContain("Question: How much memory?");
    expect(draft.body).toContain("leans towards the claim (not proof)");
    expect(draft.body).not.toMatch(/\bproven\b/);
    expect(draft.tags).toEqual(["claim check"]);
  });

  it("never exceeds the record size limit", () => {
    const long = draftFromAnswer("Big", "x".repeat(MEMORY_BODY_LIMIT + 5000));
    expect(long.body.length).toBeLessThanOrEqual(MEMORY_BODY_LIMIT);
    expect(long.body).toContain("Cut because it was too long");
  });
});

describe("saving an answer", () => {
  it("credits the page the answer was based on, and nothing else about it", () => {
    const d = draftFromAnswer("  Notes  ", "  - point one\n- point two ", { url: "https://a.example/x", title: "A page" });
    expect(d).toEqual({ kind: "page", title: "Notes", body: "- point one\n- point two", tags: ["ai"], sources: [{ url: "https://a.example/x", title: "A page" }], related: [] });
  });

  it("has no source when the answer wasn't about a page", () => {
    expect(draftFromAnswer("", "hello").sources).toEqual([]);
    expect(draftFromAnswer("", "hello").title).toBe("Saved answer");
  });

  it("reads and writes tags", () => {
    expect(parseTags("#Rust, gpu\n  RUST ,, ")).toEqual(["rust", "gpu"]);
    expect(formatTags(["a", "b"])).toBe("a, b");
    expect(parseTags(Array.from({ length: 40 }, (_, i) => `t${i}`).join(","))).toHaveLength(20);
  });
});
