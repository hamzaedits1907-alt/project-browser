import { describe, expect, it } from "vitest";

import { CITED_SUMMARY, EXPLAIN_SELECTION, LANGUAGES, defaultLanguage, translateSelection } from "./tools";

describe("ready-made assistant requests", () => {
  it("picks the browser's language for translation when it is offered, English otherwise", () => {
    expect(defaultLanguage("de-DE")).toBe("German");
    expect(defaultLanguage("pt_BR")).toBe("Portuguese");
    expect(defaultLanguage("FR")).toBe("French");
    expect(defaultLanguage("xx-YY")).toBe("English");
    expect(defaultLanguage("")).toBe("English");
    expect(LANGUAGES).toContain(defaultLanguage("ja-JP"));
  });

  it("tells the model what to do when nothing is selected", () => {
    expect(EXPLAIN_SELECTION).toContain("haven't selected");
    expect(translateSelection("German")).toContain("into German");
    expect(translateSelection("German")).toContain("haven't selected");
  });

  it("asks for a quote with every point of a source-backed summary", () => {
    expect(CITED_SUMMARY).toContain("exact words");
    expect(CITED_SUMMARY).toContain("Do not include a point you cannot quote");
  });
});
