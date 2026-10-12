import { describe, expect, it } from "vitest";

import type { ContextMenuEvent, ContextTarget, NativeMenuItem } from "../../lib/types";
import { buildMenu, findNative, flatten } from "./model";
import type { MenuContext, MenuEntry } from "./model";

let nextId = 100;
const item = (name: string, over: Partial<NativeMenuItem> = {}): NativeMenuItem => ({
  commandId: nextId++,
  name,
  label: `&${name} (de)`.replace("&", ""),
  kind: "command",
  enabled: true,
  checked: false,
  shortcut: "",
  children: [],
  ...over,
});

const target = (over: Partial<ContextTarget> = {}): ContextTarget => ({
  kind: "page",
  editable: false,
  mainFrame: true,
  pageUrl: "https://example.com/",
  frameUrl: "https://example.com/",
  linkUrl: null,
  linkText: null,
  sourceUrl: null,
  selection: null,
  ...over,
});

const event = (t: ContextTarget, items: NativeMenuItem[], over: Partial<ContextMenuEvent> = {}): ContextMenuEvent => ({
  requestId: 1,
  tabId: 5,
  x: 10,
  y: 10,
  private: false,
  target: t,
  items,
  ...over,
});

const ctx: MenuContext = { engine: "Google", tabMuted: false, canGoBack: true, canGoForward: false, canFill: false };

const keys = (entries: MenuEntry[]) => entries.map((e) => e.key);
const all = (m: ReturnType<typeof buildMenu>) => m.sections.flatMap((s) => s.entries);

describe("page right-click", () => {
  it("offers navigation, page commands and AI, using WebView2's own commands where it offered them", () => {
    const items = [item("back", { enabled: false }), item("forward", { enabled: false }), item("reload"), item("saveAs"), item("print"), item("inspectElement")];
    const m = buildMenu(event(target(), items), ctx);
    // WebView2's back was offered but disabled; ours (the tab can go back) stands in. Forward is impossible: absent.
    expect(keys(m.navRow)).toEqual(["nav-back", expect.stringMatching(/^n-reload-/)]);
    const labels = all(m).map((e) => e.label);
    expect(labels).toEqual(["Save page as…", "Print…", "Summarize this page with AI", "Translate this page with AI", "Page and site information", "Inspect"]);
    expect(all(m).find((e) => e.label === "Print…")?.native).toBe(items[4]?.commandId);
  });

  it("never offers AI or Project Memory in a private tab", () => {
    const m = buildMenu(event(target({ selection: "secret words" }), [item("copy")], { private: true }), ctx);
    const labels = all(m).map((e) => e.label);
    expect(labels.some((l) => /AI|Memory/.test(l))).toBe(false);
    expect(labels).toContain("Search Google for “secret words”");
  });
});

describe("links, images, media and fields", () => {
  it("link: our own open entries, WebView2's copy and save, our bookmark", () => {
    const copy = item("copyLinkLocation");
    const items = [item("openLinkInNewWindow"), copy, item("saveLinkAs"), item("share")];
    const m = buildMenu(event(target({ linkUrl: "https://a.example/x", linkText: "A link" }), items), ctx);
    const labels = all(m).map((e) => e.label);
    expect(labels).toEqual(["Open link in new tab", "Open link in new window", "Open link in private window", "Copy link address", "Copy link text", "Save link as…", "Bookmark this link"]);
    expect(all(m).find((e) => e.label === "Copy link address")?.native).toBe(copy.commandId);
    expect(m.title).toBe("https://a.example/x");
  });

  it("javascript: links get no open entries", () => {
    const m = buildMenu(event(target({ linkUrl: "javascript:void(0)" }), []), ctx);
    expect(all(m).some((e) => e.label.startsWith("Open link"))).toBe(false);
  });

  it("image: open, save, copy, address and visual search, but no visual search for data: images", () => {
    const items = [item("saveImageAs"), item("copyImage"), item("copyImageLocation")];
    const web = buildMenu(event(target({ kind: "image", sourceUrl: "https://cdn.example/p.png" }), items), ctx);
    expect(all(web).map((e) => e.label)).toEqual(["Open image in new tab", "Save image as…", "Copy image", "Copy image address", "Search image with Google Lens"]);
    const data = buildMenu(event(target({ kind: "image", sourceUrl: "data:image/png;base64,AA" }), []), ctx);
    expect(all(data).map((e) => e.label)).toEqual([]);
  });

  it("video: WebView2's picture-in-picture and loop (with its checked state), and the tab's mute", () => {
    const loop = item("loop", { kind: "checkbox", checked: true });
    const m = buildMenu(event(target({ kind: "video", sourceUrl: "https://v.example/a.mp4" }), [item("pictureInPicture"), loop]), ctx);
    const e = all(m);
    expect(e.map((x) => x.label).slice(0, 3)).toEqual(["Picture in picture", "Loop", "Mute tab"]);
    expect(e.find((x) => x.label === "Loop")?.checked).toBe(true);
  });

  it("editable field: WebView2's editing commands only, and the saved-login entry only when there is one", () => {
    const items = [item("undo"), item("redo", { enabled: false }), item("cut"), item("copy"), item("paste"), item("selectAll")];
    const m = buildMenu(event(target({ editable: true }), items), ctx);
    expect(all(m).map((e) => e.label)).toEqual(["Undo", "Cut", "Copy", "Paste", "Select all"]);
    expect(all(m).every((e) => e.native !== undefined)).toBe(true);
    const withFill = buildMenu(event(target({ editable: true }), items), { ...ctx, canFill: true });
    expect(all(withFill).map((e) => e.label)).toContain("Fill a saved login or address…");
  });
});

describe("honesty", () => {
  it("keeps every other offered, enabled WebView2 command under its own label and drops replaced ones", () => {
    const spell = item("spellCheck", { label: "Recieve → Receive" });
    const unknown = item("someNewCommand", { label: "Neuer Befehl" });
    const disabled = item("anotherOne", { enabled: false, label: "Off" });
    const m = buildMenu(event(target({ editable: true }), [spell, unknown, disabled, item("webCapture")]), ctx);
    const labels = all(m).map((e) => e.label);
    expect(labels).toContain("Recieve → Receive");
    expect(labels).toContain("Neuer Befehl");
    expect(labels).not.toContain("Off");
    expect(labels.some((l) => l.includes("webCapture"))).toBe(false);
  });

  it("each WebView2 command appears at most once and submenus are searched", () => {
    const nested = item("sub", { kind: "submenu", children: [item("copy")] });
    expect(flatten([nested]).map((i) => i.name)).toEqual(["copy"]);
    expect(findNative([nested], "copy")?.name).toBe("copy");
    const m = buildMenu(event(target({ selection: "x" }), [nested]), ctx);
    const natives = all(m).filter((e) => e.native !== undefined).map((e) => e.native);
    expect(new Set(natives).size).toBe(natives.length);
  });
});
