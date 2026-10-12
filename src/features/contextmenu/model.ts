// What Project Browser's right-click menu shows for a given target (pure, unit-tested).
//
// Two kinds of entries:
// * WebView2's own commands (copy, paste, save image as, picture in picture, …). They appear only when WebView2 offered
//   them for this very click and has them enabled, and choosing one hands its command id back to WebView2, which runs it.
// * The browser's own actions (open link in a new tab, search, bookmark, AI, Project Memory). They run through the
//   ordinary shell commands. AI and Project Memory never appear for private tabs.
// Every native item WebView2 offers that is not given a place below still appears, under its own (system-language)
// label, so nothing WebView2 can do is lost and nothing is invented.

import type { ContextMenuEvent, NativeMenuItem } from "../../lib/types";
import type { IconName } from "../../components/ui/Icon";

export type CustomAction =
  | { kind: "openLinkTab"; url: string }
  | { kind: "openLinkWindow"; url: string }
  | { kind: "openLinkPrivate"; url: string }
  | { kind: "copyText"; text: string; what: string }
  | { kind: "bookmarkLink"; url: string; title: string }
  | { kind: "openTab"; url: string }
  | { kind: "search"; text: string }
  | { kind: "ai"; prompt: string; withPage: boolean }
  | { kind: "aiTranslatePage" }
  | { kind: "saveToMemory"; text: string }
  | { kind: "siteInfo" }
  | { kind: "fillLogin" }
  | { kind: "muteTab"; muted: boolean }
  | { kind: "nav"; action: "back" | "forward" | "reload" };

export interface MenuEntry {
  key: string;
  label: string;
  icon?: IconName;
  hint?: string;
  checked?: boolean;
  native?: number;
  custom?: CustomAction;
}

export interface MenuSection {
  heading?: string;
  entries: MenuEntry[];
}

export interface MenuModel {
  /** A short line saying what was clicked (a link's address, the selected words). */
  title: string | null;
  /** Back / forward / reload as an icon row (page clicks only). */
  navRow: MenuEntry[];
  sections: MenuSection[];
}

export interface MenuContext {
  engine: string;
  /** The tab is muted / playing sound (for the media mute entry). */
  tabMuted: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  /** Saved logins or addresses exist for this page. */
  canFill: boolean;
}

/** Labels and icons for WebView2's commands, by their stable names. */
export const NATIVE: Record<string, { label: string; icon: IconName }> = {
  back: { label: "Back", icon: "back" },
  forward: { label: "Forward", icon: "forward" },
  reload: { label: "Reload", icon: "reload" },
  saveAs: { label: "Save page as…", icon: "save" },
  print: { label: "Print…", icon: "print" },
  inspectElement: { label: "Inspect", icon: "code" },
  copy: { label: "Copy", icon: "copy" },
  cut: { label: "Cut", icon: "cut" },
  paste: { label: "Paste", icon: "paste" },
  pasteAndMatchStyle: { label: "Paste as plain text", icon: "paste" },
  selectAll: { label: "Select all", icon: "selectAll" },
  undo: { label: "Undo", icon: "undo" },
  redo: { label: "Redo", icon: "redo" },
  emoji: { label: "Emoji", icon: "sparkle" },
  copyLinkLocation: { label: "Copy link address", icon: "link" },
  saveLinkAs: { label: "Save link as…", icon: "download" },
  copyLinkToHighlight: { label: "Copy link to highlight", icon: "link" },
  copyImage: { label: "Copy image", icon: "copy" },
  copyImageLocation: { label: "Copy image address", icon: "link" },
  saveImageAs: { label: "Save image as…", icon: "download" },
  saveMediaAs: { label: "Save media as…", icon: "download" },
  copyVideoFrame: { label: "Copy video frame", icon: "image" },
  loop: { label: "Loop", icon: "loop" },
  showAllControls: { label: "Show controls", icon: "sliders" },
  pictureInPicture: { label: "Picture in picture", icon: "pip" },
};

/** WebView2 commands left out on purpose: the browser's own entry replaces them, or they open Edge-only UI. */
export const NATIVE_REPLACED = new Set(["openLinkInNewWindow", "share", "webCapture", "createQrCode", "writingDirection", "openLinkInProfile"]);

/** Flattens WebView2's menu (submenus included). */
export function flatten(items: NativeMenuItem[]): NativeMenuItem[] {
  return items.flatMap((i) => (i.kind === "submenu" ? flatten(i.children) : [i]));
}

/** The first offered, enabled command with this name. */
export function findNative(items: NativeMenuItem[], name: string): NativeMenuItem | undefined {
  return flatten(items).find((i) => i.name === name && i.enabled && i.kind !== "separator");
}

function isWebAddress(url: string | null): url is string {
  return !!url && /^https?:\/\//i.test(url);
}

function snippet(text: string, max = 32): string {
  const one = text.replace(/\s+/g, " ").trim();
  return one.length > max ? `${one.slice(0, max - 1)}…` : one;
}

export function buildMenu(e: ContextMenuEvent, ctx: MenuContext): MenuModel {
  const t = e.target;
  const used = new Set<number>();
  const native = (name: string, extra: Partial<MenuEntry> = {}): MenuEntry | null => {
    const item = findNative(e.items, name);
    const meta = NATIVE[name];
    if (!item || used.has(item.commandId)) return null;
    used.add(item.commandId);
    return {
      key: `n-${name}-${item.commandId}`,
      label: meta?.label ?? item.label,
      icon: meta?.icon,
      hint: item.shortcut || undefined,
      checked: item.kind === "checkbox" || item.kind === "radio" ? item.checked : undefined,
      native: item.commandId,
      ...extra,
    };
  };
  const list = (...xs: (MenuEntry | null | false)[]): MenuEntry[] => xs.filter((x): x is MenuEntry => !!x);
  const sections: MenuSection[] = [];
  const add = (entries: MenuEntry[], heading?: string) => {
    if (entries.length) sections.push({ heading, entries });
  };
  const selection = t.selection?.trim() || "";
  const ai = !e.private && e.tabId !== null;
  const media = t.kind === "audio" || t.kind === "video";
  let title: string | null = null;
  let navRow: MenuEntry[] = [];

  if (isWebAddress(t.linkUrl)) {
    title = t.linkUrl;
    const url = t.linkUrl;
    add(
      list(
        { key: "link-tab", label: "Open link in new tab", icon: "plus", custom: { kind: "openLinkTab", url } },
        { key: "link-window", label: "Open link in new window", icon: "window", custom: { kind: "openLinkWindow", url } },
        !e.private && { key: "link-private", label: "Open link in private window", icon: "incognito", custom: { kind: "openLinkPrivate", url } },
      ),
    );
    add(
      list(
        native("copyLinkLocation") ?? { key: "link-copy", label: "Copy link address", icon: "link", custom: { kind: "copyText", text: url, what: "Link address" } },
        !!t.linkText?.trim() && { key: "link-text", label: "Copy link text", icon: "copy", custom: { kind: "copyText", text: t.linkText.trim(), what: "Link text" } },
        native("saveLinkAs"),
        !e.private && { key: "link-bookmark", label: "Bookmark this link", icon: "star", custom: { kind: "bookmarkLink", url, title: t.linkText?.trim() || url } },
      ),
    );
  }

  if (t.kind === "image") {
    if (!title && t.sourceUrl) title = t.sourceUrl;
    const src = t.sourceUrl;
    add(
      list(
        isWebAddress(src) && { key: "img-open", label: "Open image in new tab", icon: "image", custom: { kind: "openTab", url: src } },
        native("saveImageAs"),
        native("copyImage"),
        native("copyImageLocation") ?? (isWebAddress(src) ? { key: "img-copy-addr", label: "Copy image address", icon: "link", custom: { kind: "copyText", text: src, what: "Image address" } } : null),
        isWebAddress(src) && !e.private && { key: "img-lens", label: "Search image with Google Lens", icon: "lens", custom: { kind: "openTab", url: `https://lens.google.com/uploadbyurl?url=${encodeURIComponent(src)}` } },
      ),
      "Image",
    );
  }

  if (media) {
    const src = t.sourceUrl;
    add(
      list(
        native("pictureInPicture"),
        native("loop"),
        native("showAllControls"),
        e.tabId !== null && { key: "media-mute", label: ctx.tabMuted ? "Unmute tab" : "Mute tab", icon: ctx.tabMuted ? "volume" : "mute", custom: { kind: "muteTab", muted: !ctx.tabMuted } },
        native("copyVideoFrame"),
        native("saveMediaAs"),
        isWebAddress(src) && { key: "media-open", label: t.kind === "video" ? "Open video in new tab" : "Open audio in new tab", icon: "external", custom: { kind: "openTab", url: src } },
        isWebAddress(src) && { key: "media-copy", label: t.kind === "video" ? "Copy video address" : "Copy audio address", icon: "link", custom: { kind: "copyText", text: src, what: "Address" } },
      ),
      t.kind === "video" ? "Video" : "Audio",
    );
  }

  if (t.editable) {
    add(list(native("undo"), native("redo")));
    add(list(native("cut"), native("copy"), native("paste"), native("pasteAndMatchStyle"), native("selectAll"), native("emoji")));
    if (ctx.canFill && e.tabId !== null) add(list({ key: "fill", label: "Fill a saved login or address…", icon: "key", custom: { kind: "fillLogin" } }));
  } else if (selection) {
    if (!title) title = `“${snippet(selection, 48)}”`;
    add(
      list(
        native("copy"),
        { key: "sel-search", label: `Search ${ctx.engine} for “${snippet(selection, 24)}”`, icon: "search", custom: { kind: "search", text: selection } },
        native("copyLinkToHighlight"),
      ),
    );
    if (ai) {
      add(
        list(
          { key: "ai-explain", label: "Explain with AI", icon: "sparkle", custom: { kind: "ai", prompt: "explain", withPage: true } },
          { key: "ai-summarize", label: "Summarize with AI", icon: "sparkle", custom: { kind: "ai", prompt: "summarize", withPage: true } },
          { key: "ai-translate", label: "Translate with AI", icon: "translate", custom: { kind: "ai", prompt: "translate", withPage: true } },
          { key: "memory", label: "Save to Project Memory", icon: "brain", custom: { kind: "saveToMemory", text: selection } },
        ),
        "AI",
      );
    }
  }

  // A plain click on the page (not a link, image, field or media): page commands.
  if (!t.editable && !selection && !isWebAddress(t.linkUrl) && t.kind === "page" && e.tabId !== null) {
    navRow = list(
      native("back") ?? (ctx.canGoBack ? { key: "nav-back", label: "Back", icon: "back", custom: { kind: "nav", action: "back" } } : null),
      native("forward") ?? (ctx.canGoForward ? { key: "nav-forward", label: "Forward", icon: "forward", custom: { kind: "nav", action: "forward" } } : null),
      native("reload") ?? { key: "nav-reload", label: "Reload", icon: "reload", custom: { kind: "nav", action: "reload" } },
    );
    add(list(native("saveAs"), native("print"), native("selectAll")));
    add(
      list(
        ai && { key: "ai-page-summary", label: "Summarize this page with AI", icon: "sparkle", custom: { kind: "ai", prompt: "page", withPage: true } },
        ai && { key: "ai-page-translate", label: "Translate this page with AI", icon: "translate", custom: { kind: "aiTranslatePage" } },
        { key: "site-info", label: "Page and site information", icon: "info", custom: { kind: "siteInfo" } },
      ),
    );
  }

  // Whatever else WebView2 offered (spelling suggestions, and commands without a place above), under its own label.
  const rest = flatten(e.items).filter(
    (i) => i.enabled && i.kind !== "separator" && i.kind !== "submenu" && !used.has(i.commandId) && !NATIVE_REPLACED.has(i.name) && i.name !== "inspectElement" && i.label.trim(),
  );
  const spelling = rest.filter((i) => i.name === "spellCheck" || i.name === "other");
  const others = rest.filter((i) => !spelling.includes(i));
  add(spelling.map((i) => ({ key: `n-${i.commandId}`, label: i.label, icon: "edit" as IconName, native: i.commandId })), spelling.length ? "Spelling" : undefined);
  add(others.map((i) => ({ key: `n-${i.commandId}`, label: NATIVE[i.name]?.label ?? i.label, icon: NATIVE[i.name]?.icon, native: i.commandId, hint: i.shortcut || undefined })));
  for (const s of spelling) used.add(s.commandId);
  for (const o of others) used.add(o.commandId);

  // Developer tools last.
  add(list(native("inspectElement")));

  return { title, navRow, sections };
}
