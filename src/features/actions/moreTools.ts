// More tools for AI Browser Actions (see actions.ts for the rules every tool follows: the model only proposes a plan
// as data, every argument is checked here, the user confirms the plan, and page text is never parsed as a plan).
//
// These reach the browser through `BrowserOps`, a small set of typed shell functions (liveBrowserOps in
// actionsStore.ts). None of them touches files, passwords, cookies, forms or downloads.

import type { PanelId } from "../../lib/ui";

export type MoreToolName =
  | "search_tabs"
  | "switch_to_tab"
  | "close_tabs"
  | "close_duplicate_tabs"
  | "group_tabs_by_site"
  | "search_history"
  | "search_web"
  | "bookmark_page"
  | "add_to_reading_list"
  | "highlight_on_page"
  | "find_in_page"
  | "set_zoom"
  | "dark_mode_page"
  | "take_screenshot"
  | "open_reader"
  | "mute_tab"
  | "start_focus"
  | "save_note"
  | "open_panel"
  | "list_page_media"
  | "answer_about_page"
  | "translate_page"
  | "write_text"
  | "copy_to_clipboard";

export type MoreEffect = "loads-web" | "sends-page" | "changes-view" | "changes-tabs" | "closes-tabs" | "saves-memory" | "uses-ai" | "none";

export interface TabBrief {
  id: number;
  title: string;
  url: string;
  pinned: boolean;
  audible: boolean;
  private: boolean;
}

export interface BrowserOps {
  tabs: () => TabBrief[];
  activate: (id: number) => Promise<void>;
  close: (ids: number[]) => Promise<number>;
  closeDuplicates: () => Promise<number>;
  groupBySite: () => Promise<number>;
  historySearch: (query: string) => Promise<{ title: string; url: string; visitedAt: number }[]>;
  historyText: (query: string) => Promise<{ title: string; url: string; snippet: string }[]>;
  search: (query: string, isPrivate: boolean) => Promise<void>;
  bookmarkActive: () => Promise<void>;
  readingList: (url: string, title: string) => Promise<void>;
  highlight: (tabId: number, url: string, title: string, text: string) => Promise<void>;
  find: (text: string) => Promise<void>;
  zoom: (tabId: number, factor: number) => Promise<void>;
  darkPage: (tabId: number, on: boolean) => Promise<void>;
  screenshot: (full: boolean) => Promise<void>;
  reader: () => Promise<void>;
  mute: (tabId: number, on: boolean) => Promise<void>;
  focus: (minutes: number) => void;
  note: (text: string) => void;
  openPanel: (panel: PanelId) => void;
  copy: (text: string) => Promise<void>;
  pageMedia: (tabId: number) => Promise<{ kind: string; url: string; width: number; height: number }[]>;
}

type Check = { ok: true; args: Record<string, unknown> } | { ok: false; error: string };
const ok = (args: Record<string, unknown>): Check => ({ ok: true, args });
const bad = (error: string): Check => ({ ok: false, error });

function text(v: unknown, min: number, max: number): string | null {
  if (typeof v !== "string") return null;
  const t = v.replace(/\s+/g, " ").trim();
  return t.length >= min && t.length <= max ? t : null;
}

const PANELS: PanelId[] = ["bookmarks", "reading", "history", "downloads", "shield", "notes", "media", "focus", "highlights", "pagemedia"];

export interface MoreToolSpec {
  effect: MoreEffect;
  help: string;
  /** Needs a (non-private) web page as the current tab. */
  needsPage?: boolean;
  check: (args: Record<string, unknown>) => Check;
  describe: (args: Record<string, unknown>) => string;
}

const query = (field: string, label: string) => (a: Record<string, unknown>) => {
  const q = text(a[field], 1, 200);
  return q ? ok({ [field]: q }) : bad(`${label} is needed`);
};

export const MORE_TOOLS: Record<MoreToolName, MoreToolSpec> = {
  search_tabs: {
    effect: "none",
    help: 'args: { "query": "words" }. Lists open tabs whose title or address match.',
    check: query("query", "a search"),
    describe: (a) => `Find open tabs about “${a.query as string}”`,
  },
  switch_to_tab: {
    effect: "changes-tabs",
    help: 'args: { "query": "words" }. Switches to the open tab that matches best.',
    check: query("query", "a search"),
    describe: (a) => `Switch to the tab about “${a.query as string}”`,
  },
  close_tabs: {
    effect: "closes-tabs",
    help: 'args: { "query": "words" }. Closes the open tabs whose title or address match (never pinned tabs or the current one). Ctrl+Shift+T brings them back.',
    check: query("query", "a search"),
    describe: (a) => `Close tabs about “${a.query as string}”`,
  },
  close_duplicate_tabs: {
    effect: "closes-tabs",
    help: "args: {}. Closes tabs that show the same page as another tab.",
    check: () => ok({}),
    describe: () => "Close duplicate tabs",
  },
  group_tabs_by_site: {
    effect: "changes-tabs",
    help: "args: {}. Puts tabs of the same site into tab groups.",
    check: () => ok({}),
    describe: () => "Group tabs by site",
  },
  search_history: {
    effect: "none",
    help: 'args: { "query": "words" }. Searches the user\'s history by title and address, and inside the text of visited pages.',
    check: query("query", "a search"),
    describe: (a) => `Search history for “${a.query as string}”`,
  },
  search_web: {
    effect: "loads-web",
    help: 'args: { "query": "words" }. Searches the web with the user\'s search engine in a new tab.',
    check: query("query", "a search"),
    describe: (a) => `Search the web for “${a.query as string}”`,
  },
  bookmark_page: {
    effect: "saves-memory",
    help: "args: {}. Bookmarks the current page.",
    needsPage: true,
    check: () => ok({}),
    describe: () => "Bookmark this page",
  },
  add_to_reading_list: {
    effect: "saves-memory",
    help: "args: {}. Saves the current page to the Reading List.",
    needsPage: true,
    check: () => ok({}),
    describe: () => "Add this page to the Reading List",
  },
  highlight_on_page: {
    effect: "changes-view",
    help: 'args: { "text": "exact words from the page" }. Highlights those words on the current page (kept for next visits). Use text from an earlier step\'s answer.',
    needsPage: true,
    check: (a) => {
      const t = text(a.text, 2, 500);
      return t ? ok({ text: t }) : bad("the words to highlight are needed");
    },
    describe: (a) => `Highlight “${(a.text as string).slice(0, 60)}” on this page`,
  },
  find_in_page: {
    effect: "changes-view",
    help: 'args: { "text": "words" }. Finds the words on the current page (like Ctrl+F).',
    needsPage: true,
    check: (a) => {
      const t = text(a.text, 1, 200);
      return t ? ok({ text: t }) : bad("the words to find are needed");
    },
    describe: (a) => `Find “${a.text as string}” on this page`,
  },
  set_zoom: {
    effect: "changes-view",
    help: 'args: { "percent": 25 to 500 }. Sets the current page\'s zoom.',
    needsPage: true,
    check: (a) => {
      const p = typeof a.percent === "number" ? a.percent : Number(a.percent);
      return Number.isFinite(p) && p >= 25 && p <= 500 ? ok({ percent: Math.round(p) }) : bad("the zoom must be between 25 and 500 percent");
    },
    describe: (a) => `Zoom this page to ${a.percent as number}%`,
  },
  dark_mode_page: {
    effect: "changes-view",
    help: 'args: { "on": true or false }. Shows the current site dark (or not), remembered for the site.',
    needsPage: true,
    check: (a) => (typeof a.on === "boolean" ? ok({ on: a.on }) : bad('"on" must be true or false')),
    describe: (a) => ((a.on as boolean) ? "Show this site dark" : "Show this site in its own colours"),
  },
  take_screenshot: {
    effect: "none",
    help: 'args: { "full": true for the whole page, false for what is visible }. Opens the screenshot tool (the user saves or copies it).',
    needsPage: true,
    check: (a) => ok({ full: a.full === true }),
    describe: (a) => ((a.full as boolean) ? "Screenshot of the whole page" : "Screenshot of what you see"),
  },
  open_reader: {
    effect: "changes-view",
    help: "args: {}. Opens Reader view (clean text, with Listen to read it aloud).",
    needsPage: true,
    check: () => ok({}),
    describe: () => "Open Reader view",
  },
  mute_tab: {
    effect: "changes-view",
    help: 'args: { "on": true to mute, false to unmute }. Mutes the current tab.',
    check: (a) => (typeof a.on === "boolean" ? ok({ on: a.on }) : bad('"on" must be true or false')),
    describe: (a) => ((a.on as boolean) ? "Mute this tab" : "Unmute this tab"),
  },
  start_focus: {
    effect: "changes-view",
    help: 'args: { "minutes": 5 to 240 }. Starts a Focus session that blocks distracting sites.',
    check: (a) => {
      const m = typeof a.minutes === "number" ? a.minutes : Number(a.minutes);
      return Number.isFinite(m) && m >= 5 && m <= 240 ? ok({ minutes: Math.round(m) }) : bad("minutes must be between 5 and 240");
    },
    describe: (a) => `Start a ${a.minutes as number}-minute Focus session`,
  },
  save_note: {
    effect: "saves-memory",
    help: 'args: { "text": "optional; if left out the previous step\'s result is saved" }. Adds text to the Notes panel.',
    check: (a) => {
      if (a.text === undefined || a.text === null || a.text === "") return ok({});
      const t = typeof a.text === "string" ? a.text.trim() : "";
      return t && t.length <= 20_000 ? ok({ text: t }) : bad("the note is empty or too long");
    },
    describe: () => "Add to Notes",
  },
  open_panel: {
    effect: "changes-view",
    help: `args: { "panel": one of ${PANELS.map((p) => `"${p}"`).join(", ")} }. Opens a side panel.`,
    check: (a) => (typeof a.panel === "string" && (PANELS as string[]).includes(a.panel) ? ok({ panel: a.panel }) : bad("that panel doesn't exist")),
    describe: (a) => `Open the ${a.panel as string} panel`,
  },
  list_page_media: {
    effect: "none",
    help: "args: {}. Lists the pictures, videos and sounds on the current page (addresses and sizes).",
    needsPage: true,
    check: () => ok({}),
    describe: () => "List the pictures and videos on this page",
  },
  answer_about_page: {
    effect: "sends-page",
    help: 'args: { "question": "the question" }. Answers a question about the current page (sends its text to the model).',
    needsPage: true,
    check: (a) => {
      const q = text(a.question, 3, 500);
      return q ? ok({ question: q }) : bad("a question is needed");
    },
    describe: (a) => `Answer from this page: ${a.question as string}`,
  },
  translate_page: {
    effect: "sends-page",
    help: 'args: { "language": "English, German, ..." }. Translates the main text of the current page (sends its text to the model).',
    needsPage: true,
    check: (a) => {
      const l = text(a.language, 2, 40);
      return l && /^[\p{L} ()-]+$/u.test(l) ? ok({ language: l }) : bad("say which language");
    },
    describe: (a) => `Translate this page into ${a.language as string}`,
  },
  write_text: {
    effect: "uses-ai",
    help: 'args: { "instructions": "what to write, e.g. a polite reply declining the meeting" }. Writes text (an email, a message, a list). Uses the previous step\'s result as material when there is one. No page is sent.',
    check: (a) => {
      const t = text(a.instructions, 3, 1000);
      return t ? ok({ instructions: t }) : bad("say what to write");
    },
    describe: (a) => `Write: ${(a.instructions as string).slice(0, 80)}`,
  },
  copy_to_clipboard: {
    effect: "none",
    help: 'args: { "text": "optional; if left out the previous step\'s result is copied" }. Copies text to the clipboard.',
    check: (a) => {
      if (a.text === undefined || a.text === null || a.text === "") return ok({});
      const t = typeof a.text === "string" ? a.text : "";
      return t && t.length <= 50_000 ? ok({ text: t }) : bad("the text is empty or too long");
    },
    describe: () => "Copy to the clipboard",
  },
};

export const MORE_TOOL_NAMES = Object.keys(MORE_TOOLS) as MoreToolName[];

/** Open tabs matching all the words, best first. Pure. */
export function matchTabs(tabs: TabBrief[], q: string): TabBrief[] {
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  const score = (t: TabBrief) => {
    const hay = `${t.title} ${t.url}`.toLowerCase();
    if (!words.every((w) => hay.includes(w))) return 0;
    return words.reduce((n, w) => n + (t.title.toLowerCase().includes(w) ? 2 : 1), 0);
  };
  return tabs
    .map((t) => ({ t, s: score(t) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s)
    .map((x) => x.t);
}

export interface MoreContext {
  ops: BrowserOps;
  active: { id: number; url: string; title: string; private: boolean; web: boolean } | null;
  previous: string | null;
  /** Reads the current page and asks the model (sends-page tools). */
  askPage: (request: string) => Promise<string>;
  /** Asks the model without any page (write_text). */
  askPlain: (request: string) => Promise<string>;
}

const lines = <T,>(xs: T[], f: (x: T) => string, none: string) => (xs.length ? xs.map(f).join("\n\n") : none);

/** Runs one of the tools above. Returns the step's text (shown, never interpreted). Throws with a plain reason. */
export async function runMoreTool(tool: MoreToolName, a: Record<string, unknown>, c: MoreContext): Promise<string> {
  const page = () => {
    const t = c.active;
    if (!t || !t.web) throw new Error("There is no web page open to work on.");
    if (t.private) throw new Error("The assistant never reads or changes private tabs.");
    return t;
  };
  switch (tool) {
    case "search_tabs":
      return lines(matchTabs(c.ops.tabs(), a.query as string).slice(0, 15), (t) => `${t.title || t.url}\n${t.url}`, "No open tab matches.");
    case "switch_to_tab": {
      const hit = matchTabs(c.ops.tabs(), a.query as string)[0];
      if (!hit) throw new Error("No open tab matches.");
      await c.ops.activate(hit.id);
      return `Switched to “${hit.title || hit.url}”.`;
    }
    case "close_tabs": {
      const ids = matchTabs(c.ops.tabs(), a.query as string)
        .filter((t) => !t.pinned && t.id !== c.active?.id)
        .map((t) => t.id);
      if (ids.length === 0) return "No tab to close matches (pinned tabs and the current tab are never closed).";
      const n = await c.ops.close(ids);
      return `Closed ${n} tab${n === 1 ? "" : "s"}. Ctrl+Shift+T brings them back.`;
    }
    case "close_duplicate_tabs": {
      const n = await c.ops.closeDuplicates();
      return n ? `Closed ${n} duplicate tab${n === 1 ? "" : "s"}.` : "There were no duplicate tabs.";
    }
    case "group_tabs_by_site": {
      const n = await c.ops.groupBySite();
      return n ? `Made ${n} group${n === 1 ? "" : "s"}.` : "No site had more than one tab.";
    }
    case "search_history": {
      const q = a.query as string;
      const [plain, inside] = await Promise.all([c.ops.historySearch(q), c.ops.historyText(q).catch(() => [])]);
      const seen = new Set<string>();
      const out: string[] = [];
      for (const h of inside.slice(0, 8)) {
        seen.add(h.url);
        out.push(`${h.title || h.url}\n${h.url}\n“${h.snippet.split(String.fromCharCode(2)).join("").split(String.fromCharCode(3)).join("")}”`);
      }
      for (const h of plain) {
        if (seen.has(h.url) || out.length >= 15) continue;
        seen.add(h.url);
        out.push(`${h.title || h.url}\n${h.url}`);
      }
      return out.length ? out.join("\n\n") : "Nothing in history matches.";
    }
    case "search_web":
      await c.ops.search(a.query as string, !!c.active?.private);
      return `Searched for “${a.query as string}” in a new tab.`;
    case "bookmark_page":
      page();
      await c.ops.bookmarkActive();
      return "Bookmarked.";
    case "add_to_reading_list": {
      const t = page();
      await c.ops.readingList(t.url, t.title);
      return "Saved to the Reading List.";
    }
    case "highlight_on_page": {
      const t = page();
      await c.ops.highlight(t.id, t.url, t.title, a.text as string);
      return "Highlighted.";
    }
    case "find_in_page":
      page();
      await c.ops.find(a.text as string);
      return `Looking for “${a.text as string}” on the page.`;
    case "set_zoom": {
      const t = page();
      await c.ops.zoom(t.id, (a.percent as number) / 100);
      return `Zoom set to ${a.percent as number}%.`;
    }
    case "dark_mode_page": {
      const t = page();
      await c.ops.darkPage(t.id, a.on as boolean);
      return (a.on as boolean) ? "This site is shown dark now." : "This site is shown in its own colours now.";
    }
    case "take_screenshot":
      page();
      await c.ops.screenshot(a.full as boolean);
      return "The screenshot tool is open.";
    case "open_reader":
      page();
      await c.ops.reader();
      return "Reader view is open.";
    case "mute_tab": {
      const t = c.active;
      if (!t) throw new Error("No tab is open.");
      await c.ops.mute(t.id, a.on as boolean);
      return (a.on as boolean) ? "Muted." : "Unmuted.";
    }
    case "start_focus":
      c.ops.focus(a.minutes as number);
      return `Focus session started for ${a.minutes as number} minutes.`;
    case "save_note": {
      const body = (a.text as string | undefined) ?? c.previous;
      if (!body) throw new Error("There is nothing from an earlier step to save.");
      c.ops.note(body);
      return "Added to Notes.";
    }
    case "open_panel":
      c.ops.openPanel(a.panel as PanelId);
      return "Opened.";
    case "list_page_media": {
      const t = page();
      const media = await c.ops.pageMedia(t.id);
      return lines(media.slice(0, 30), (m) => `${m.kind}${m.width ? ` ${m.width}×${m.height}` : ""}: ${m.url}`, "No pictures, videos or sounds found.");
    }
    case "answer_about_page":
      page();
      return c.askPage(`Answer this question using only the page: ${a.question as string}`);
    case "translate_page":
      page();
      return c.askPage(`Translate the main text of this page into ${a.language as string}. Keep headings and lists. Leave out menus, ads and footers.`);
    case "write_text": {
      const material = c.previous ? `\n\nMaterial from the previous step (use it as needed):\n${c.previous.slice(0, 8000)}` : "";
      return c.askPlain(`Write the following. Give only the text itself, ready to use.\n\n${a.instructions as string}${material}`);
    }
    case "copy_to_clipboard": {
      const body = (a.text as string | undefined) ?? c.previous;
      if (!body) throw new Error("There is nothing from an earlier step to copy.");
      await c.ops.copy(body);
      return `Copied ${body.length.toLocaleString()} characters.`;
    }
  }
}
