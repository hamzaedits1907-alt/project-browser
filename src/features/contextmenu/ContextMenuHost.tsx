import { useCallback, useEffect, useRef, useState } from "react";

import { Icon } from "../../components/ui/Icon";
import { MenuHeading, MenuItem, MenuSeparator, Popover, usePopoverClose } from "../../components/ui/Popover";
import { BAR_ID } from "../../lib/bookmarks";
import { api, on } from "../../lib/ipc";
import { overlayIdle } from "../../lib/overlay";
import { engineLabel } from "../../lib/settings";
import { cue } from "../../lib/sounds";
import { attempt, emitBus, getState, toast } from "../../lib/store";
import type { ContextMenuEvent } from "../../lib/types";
import { canOfferFill } from "../../lib/vault";
import { sendMessage, setAiOpen, setIncludePage } from "../ai/aiStore";
import { EXPLAIN_SELECTION, defaultLanguage, translateSelection } from "../ai/tools";
import { draftFromAnswer } from "../memory/memory";
import { buildMenu } from "./model";
import type { CustomAction, MenuEntry, MenuModel } from "./model";

function copy(text: string, what: string): void {
  navigator.clipboard
    .writeText(text)
    .then(() => toast(`${what} copied`))
    .catch(() => toast(`Could not copy the ${what.toLowerCase()}`));
}

function askAi(prompt: string): void {
  const tab = getState().browser.tabs.find((t) => t.id === getState().browser.activeId);
  setAiOpen(true);
  setIncludePage(true);
  void sendMessage(prompt, true, tab ? { url: tab.url, title: tab.title } : undefined);
}

function runCustom(a: CustomAction, e: ContextMenuEvent): void {
  const isPrivate = e.private;
  switch (a.kind) {
    case "openLinkTab":
      void attempt(() => api.tabNew(a.url, isPrivate, true));
      return;
    case "openLinkWindow":
      void attempt(() => api.windowNew(a.url, isPrivate));
      return;
    case "openLinkPrivate":
      void attempt(() => api.windowNew(a.url, true));
      return;
    case "openTab":
      void attempt(() => api.tabNew(a.url, isPrivate));
      return;
    case "copyText":
      copy(a.text, a.what);
      return;
    case "bookmarkLink":
      void attempt(() => api.bookmarkCreate(BAR_ID, a.title.slice(0, 200), a.url).then(() => toast("Link added to the bookmarks bar")));
      return;
    case "search":
      void attempt(() => api.tabNew(a.text.slice(0, 500), isPrivate));
      return;
    case "ai": {
      const lang = defaultLanguage(navigator.language);
      const prompt =
        a.prompt === "explain"
          ? EXPLAIN_SELECTION
          : a.prompt === "summarize"
            ? "Summarize the text I selected on the page in a few short bullet points. If I haven't selected any text on the page, say so and stop."
            : a.prompt === "translate"
              ? translateSelection(lang)
              : "Summarize this page in a few bullet points.";
      askAi(prompt);
      return;
    }
    case "aiTranslatePage":
      askAi(`Translate the main text of this page into ${defaultLanguage(navigator.language)}. Keep headings and lists.`);
      return;
    case "saveToMemory": {
      const tab = getState().browser.tabs.find((t) => t.id === e.tabId);
      const words = a.text.replace(/\s+/g, " ").trim();
      const draft = { ...draftFromAnswer(words.slice(0, 80), a.text, tab ? { url: tab.url, title: tab.title } : undefined), kind: "note" as const, tags: ["selection"] };
      void attempt(() => api.memorySave(draft), "Could not save").then((saved) => saved && toast("Saved to Project Memory"));
      return;
    }
    case "siteInfo":
      emitBus("openSiteInfo");
      return;
    case "fillLogin":
      emitBus("openFill");
      return;
    case "muteTab":
      if (e.tabId !== null) {
        const id = e.tabId;
        void attempt(() => api.tabMute(id, a.muted));
      }
      return;
    case "nav":
      void attempt(() => api.nav(a.action, e.tabId ?? undefined));
      return;
  }
}

function NavRow({ entries, onPick }: { entries: MenuEntry[]; onPick: (e: MenuEntry) => void }) {
  const close = usePopoverClose();
  if (entries.length === 0) return null;
  return (
    <div className="ctx-nav" role="group" aria-label="Navigation">
      {entries.map((n) => (
        <button
          key={n.key}
          type="button"
          role="menuitem"
          className="ctx-nav-btn"
          title={n.label}
          aria-label={n.label}
          onClick={() => {
            cue("tap");
            close();
            onPick(n);
          }}
        >
          {n.icon && <Icon name={n.icon} size={17} />}
        </button>
      ))}
    </div>
  );
}

/**
 * Project Browser's right-click menu for web pages and the shell's text fields (see native/context_menu.rs). The page
 * behind it stays as it was (the backdrop is not dimmed). Every request WebView2 raised is completed exactly once:
 * with the command the user picked, or with none.
 */
export function ContextMenuHost() {
  const [menu, setMenu] = useState<{ event: ContextMenuEvent; model: MenuModel } | null>(null);
  const [open, setOpen] = useState(false);
  // What to do once the menu has closed and the page is back: a WebView2 command id, or nothing.
  const pending = useRef<{ requestId: number; command: number | null } | null>(null);
  const answered = useRef(new Set<number>());
  const shownId = useRef<number | null>(null);

  const answer = useCallback((requestId: number, command: number | null) => {
    if (answered.current.has(requestId)) return;
    answered.current.add(requestId);
    void api.contextMenuChoose(requestId, command).catch(() => {});
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void on("pb:contextmenu", (event) => {
      const s = getState();
      const tab = s.browser.tabs.find((t) => t.id === event.tabId) ?? null;
      const model = buildMenu(event, {
        engine: engineLabel(s.settings),
        tabMuted: !!tab?.muted,
        canGoBack: !!tab?.canGoBack,
        canGoForward: !!tab?.canGoForward,
        canFill: !!tab && canOfferFill(tab) && event.target.editable,
      });
      if (model.navRow.length === 0 && model.sections.length === 0) {
        answer(event.requestId, null);
        return;
      }
      // A newer right-click replaces a menu that is still open; the old request is completed without a command.
      if (shownId.current !== null && shownId.current !== event.requestId) answer(shownId.current, null);
      shownId.current = event.requestId;
      setMenu({ event, model });
      pending.current = { requestId: event.requestId, command: null };
      setOpen(true);
    }).then((u) => {
      if (disposed) u();
      else unlisten = u;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [answer]);

  const close = useCallback(() => setOpen(false), []);

  const pick = useCallback(
    (entry: MenuEntry) => {
      if (!menu) return;
      if (entry.native !== undefined) {
        // WebView2 runs its own command once the page is visible and focused again (see onExited).
        pending.current = { requestId: menu.event.requestId, command: entry.native };
        return;
      }
      answer(menu.event.requestId, null);
      if (entry.custom) runCustom(entry.custom, menu.event);
    },
    [menu, answer],
  );

  const onExited = useCallback(() => {
    const p = pending.current;
    pending.current = null;
    if (!p) return;
    // The page is being given back (overlay closing); run the command when that has finished.
    void overlayIdle().then(() => answer(p.requestId, p.command));
  }, [answer]);

  const model = menu?.model;
  return (
    <Popover
      open={open && !!menu}
      onClose={close}
      onExited={onExited}
      point={menu ? { x: menu.event.x, y: menu.event.y } : null}
      width={286}
      label="Page menu"
      className="menu ctx-menu"
      clearBackdrop
    >
      {model && (
        <div role="menu">
          {model.title && (
            <div className="ctx-title truncate" title={model.title} data-selectable>
              {model.title}
            </div>
          )}
          <NavRow entries={model.navRow} onPick={pick} />
          {model.sections.map((s, i) => (
            <div key={i} role="group">
              {(i > 0 || model.navRow.length > 0 || model.title) && <MenuSeparator />}
              {s.heading && <MenuHeading>{s.heading}</MenuHeading>}
              {s.entries.map((e) => (
                <MenuItem key={e.key} icon={e.icon} label={e.label} hint={e.hint} checked={e.checked} onSelect={() => pick(e)} />
              ))}
            </div>
          ))}
        </div>
      )}
    </Popover>
  );
}
