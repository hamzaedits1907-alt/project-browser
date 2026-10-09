import { describe, expect, it } from "vitest";

import { parseBlocks, parseSpans } from "./markdown";

describe("assistant markdown", () => {
  it("parses inline code and bold", () => {
    expect(parseSpans("use `cargo build` and **care**")).toEqual([
      { kind: "text", text: "use " },
      { kind: "code", text: "cargo build" },
      { kind: "text", text: " and " },
      { kind: "bold", text: "care" },
    ]);
  });
  it("never produces markup: hostile text stays plain text", () => {
    const hostile = "<img src=x onerror=alert(1)> <script>x</script>";
    expect(parseSpans(hostile)).toEqual([{ kind: "text", text: hostile }]);
  });
  it("parses fences, lists, headings and paragraphs", () => {
    const blocks = parseBlocks("# Title\n\nIntro line\ncontinues\n\n- a\n- b\n\n1. one\n2. two\n\n```rust\nfn main() {}\n```\nafter");
    expect(blocks.map((b) => b.type)).toEqual(["heading", "para", "list", "list", "code", "para"]);
    expect(blocks[4]).toMatchObject({ type: "code", lang: "rust", text: "fn main() {}" });
    expect(blocks[2]).toMatchObject({ type: "list", ordered: false });
    expect(blocks[3]).toMatchObject({ type: "list", ordered: true });
  });
  it("tolerates a code fence that is still streaming", () => {
    const blocks = parseBlocks("text\n```js\nlet x = 1;");
    expect(blocks.at(-1)).toMatchObject({ type: "code", text: "let x = 1;" });
  });
  it("handles empty input", () => {
    expect(parseBlocks("")).toEqual([]);
  });
});
