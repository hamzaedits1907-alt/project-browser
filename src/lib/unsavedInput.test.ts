import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// The script the core runs inside a page before unloading its tab. It runs in WebView2, so here it is evaluated
// against a minimal fake DOM that implements exactly the properties it reads.
const script = readFileSync(join(process.cwd(), "src-tauri/src/native/unsaved_input.js"), "utf8");

interface FakeEl {
  tagName: string;
  type?: string;
  value?: string;
  defaultValue?: string;
  checked?: boolean;
  defaultChecked?: boolean;
  multiple?: boolean;
  options?: { selected: boolean; defaultSelected: boolean }[];
  selectedIndex?: number;
  isContentEditable?: boolean;
  textContent?: string;
  editableAttr?: string;
}

function ask(elements: FakeEl[] | "throws"): unknown {
  const doc = {
    querySelectorAll: () => {
      if (elements === "throws") throw new Error("page is broken");
      return elements.map((e) => ({ ...e, matches: (sel: string) => sel === '[contenteditable="false"]' && e.editableAttr === "false" }));
    },
  };
  return new Function("document", `return ${script}`)(doc);
}

const input = (value: string, defaultValue: string, type = "text"): FakeEl => ({ tagName: "INPUT", type, value, defaultValue });
const opts = (...states: [boolean, boolean][]) => states.map(([selected, defaultSelected]) => ({ selected, defaultSelected }));

describe("unsaved-input check (runs before a tab is unloaded)", () => {
  it("is false for a page with no form controls", () => {
    expect(ask([])).toBe(false);
  });

  it("is true when a text field was changed, false when it still has its original value", () => {
    expect(ask([input("hello", "")])).toBe(true);
    expect(ask([input("", "")])).toBe(false);
    expect(ask([input("prefilled", "prefilled")])).toBe(false);
  });

  it("ignores hidden fields and buttons", () => {
    expect(ask([input("x", "", "hidden"), input("Go", "", "submit"), input("", "y", "button"), input("c:/f", "", "file")])).toBe(false);
  });

  it("notices a toggled checkbox or radio button", () => {
    expect(ask([{ tagName: "INPUT", type: "checkbox", checked: true, defaultChecked: false }])).toBe(true);
    expect(ask([{ tagName: "INPUT", type: "radio", checked: false, defaultChecked: false }])).toBe(false);
  });

  it("notices a changed textarea", () => {
    expect(ask([{ tagName: "TEXTAREA", value: "draft", defaultValue: "" }])).toBe(true);
    expect(ask([{ tagName: "TEXTAREA", value: "", defaultValue: "" }])).toBe(false);
  });

  it("does not mistake an untouched select for typed-in data (a select with no `selected` attribute shows its first option)", () => {
    // Regression: the first option is `selected` although `defaultSelected` is false for every option.
    const untouched: FakeEl = { tagName: "SELECT", multiple: false, options: opts([true, false], [false, false], [false, false]), selectedIndex: 0 };
    expect(ask([untouched])).toBe(false);
  });

  it("notices a select the user changed, and respects an explicit default", () => {
    expect(ask([{ tagName: "SELECT", multiple: false, options: opts([false, false], [true, false]), selectedIndex: 1 }])).toBe(true);
    expect(ask([{ tagName: "SELECT", multiple: false, options: opts([false, false], [false, false], [true, true]), selectedIndex: 2 }])).toBe(false);
    expect(ask([{ tagName: "SELECT", multiple: true, options: opts([true, false], [false, false]), selectedIndex: 0 }])).toBe(true);
    expect(ask([{ tagName: "SELECT", multiple: true, options: opts([true, true], [false, false]), selectedIndex: 0 }])).toBe(false);
  });

  it("counts a rich-text box with content, but not an empty one or one marked contenteditable=false", () => {
    expect(ask([{ tagName: "DIV", isContentEditable: true, textContent: "  my notes " }])).toBe(true);
    expect(ask([{ tagName: "DIV", isContentEditable: true, textContent: "   " }])).toBe(false);
    expect(ask([{ tagName: "DIV", isContentEditable: true, textContent: "locked", editableAttr: "false" }])).toBe(false);
  });

  it("answers yes when the page cannot be inspected (nothing may be thrown away on a doubt)", () => {
    expect(ask("throws")).toBe(true);
  });

  it("one changed field among untouched ones is enough", () => {
    expect(ask([input("", ""), { tagName: "SELECT", multiple: false, options: opts([true, false]), selectedIndex: 0 }, input("typed", "")])).toBe(true);
  });
});
