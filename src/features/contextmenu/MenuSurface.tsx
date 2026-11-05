import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";

import { Icon } from "../../components/ui/Icon";
import { MenuHeading, MenuItem, MenuSeparator, PopoverContext } from "../../components/ui/Popover";
import { api } from "../../lib/ipc";
import { applyCachedLook } from "../../lib/theme";
import { MENU_DISMISS, MENU_PICK, MENU_SHOW } from "./surface";
import type { MenuPick, MenuShow } from "./surface";

const WIDTH = 288;

const allRows = (model: NonNullable<MenuShow["model"]>) => model.sections.flatMap((s) => s.entries);

function items(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled), [role="menuitemcheckbox"]:not(:disabled)')];
}

/** Where the keyboard highlight goes for `key` from `at` (-1 = none yet) in a list of `n` entries. Pure. */
export function nextHot(key: string, at: number, n: number): number {
  if (n === 0) return -1;
  if (key === "Home") return 0;
  if (key === "End") return n - 1;
  if (key === "ArrowDown") return (at + 1) % n;
  if (key === "ArrowUp") return at <= 0 ? n - 1 : at - 1;
  return at;
}

/**
 * The right-click menu's own window. It renders the model a shell sent, sizes and shows itself at the click (the core
 * places it), and reports the pick back to that shell. It never runs an action itself and holds no browser state.
 *
 * It never has keyboard focus (menu_surface.rs: a non-activating window), so nothing here depends on focus events:
 * the core closes it on a click outside, on Escape (keys typed into the page are routed here as `pb:menu-key`) and when
 * the browser window goes to the background (`pb:menu-dismiss`). Every menu ends exactly once, through `finish`.
 */
export function MenuSurface() {
  const [menu, setMenu] = useState<MenuShow | null>(null);
  const current = useRef<MenuShow | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const hot = useRef(-1);

  const finish = useCallback((key: string | null) => {
    const m = current.current;
    if (!m) return;
    current.current = null;
    hot.current = -1;
    void api.menuDismiss().catch(() => {});
    const pick: MenuPick = { requestId: m.requestId, key };
    void emitTo(m.owner, MENU_PICK, pick).catch(() => {});
    setMenu(null);
  }, []);

  const highlight = useCallback((index: number) => {
    const el = root.current;
    if (!el) return;
    const list = items(el);
    list.forEach((item, i) => item.classList.toggle("is-hot", i === index));
    hot.current = index;
  }, []);

  useEffect(() => {
    document.documentElement.classList.add("is-menu-surface");
    const unlisten: Array<() => void> = [];
    let disposed = false;
    const keep = (u: () => void) => (disposed ? u() : unlisten.push(u));
    void listen<MenuShow>(MENU_SHOW, (e) => {
      // A newer menu replaces one still open: the old one is answered as dismissed.
      if (current.current && current.current.requestId !== e.payload.requestId) finish(null);
      applyCachedLook();
      const same = current.current?.requestId === e.payload.requestId;
      current.current = e.payload;
      if (!same) hot.current = -1;
      setMenu(e.payload);
    }).then(keep);
    void listen<string | { requestId: number }>(MENU_DISMISS, (e) => {
      // A shell closing its own popup names it: a newer menu shown meanwhile stays open.
      if (typeof e.payload === "object" && e.payload && current.current?.requestId !== e.payload.requestId) return;
      finish(null);
    }).then(keep);
    void listen<string>("pb:menu-key", (e) => {
      const el = root.current;
      if (!el || !current.current) return;
      if (e.payload === "Escape") return finish(null);
      const list = items(el);
      if (e.payload === "Enter") {
        if (hot.current >= 0) list[hot.current]?.click();
        return;
      }
      highlight(nextHot(e.payload, hot.current, list.length));
    }).then(keep);
    const block = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", block);
    void api.menuSurfaceReady().catch(() => {});
    return () => {
      disposed = true;
      unlisten.forEach((u) => u());
      document.removeEventListener("contextmenu", block);
    };
  }, [finish, highlight]);

  // Rendered: measure and show at the click. The window has the exact size of the menu.
  useLayoutEffect(() => {
    const el = root.current;
    if (!menu || !el) return;
    const r = el.getBoundingClientRect();
    if (menu.hot !== undefined) highlight(menu.hot);
    // The full content height (the window may still have the previous, smaller menu's size).
    void api.menuPresent(Math.ceil(r.width), Math.ceil(Math.max(r.height, el.scrollHeight))).catch(() => finish(null));
  }, [menu, finish, highlight]);

  const model = menu?.model;
  if (menu?.variant === "list" && model) {
    const rows = allRows(model);
    return (
      <div ref={root} role="listbox" aria-label="Suggestions" className="popover menu-surface surface-list" style={{ width: menu.width ?? WIDTH }}>
        {rows.map((entry) => (
          <button key={entry.key} type="button" role="menuitem" className="surface-row" onClick={() => finish(entry.key)}>
            <span className="surface-row-icon">{entry.icon && <Icon name={entry.icon} size={15} />}</span>
            <span className="surface-row-text">
              <span className="surface-row-title truncate">{entry.label}</span>
              {entry.detail && <span className="surface-row-detail truncate">{entry.detail}</span>}
            </span>
            {entry.hint && <span className="surface-row-hint">{entry.hint}</span>}
          </button>
        ))}
        <div className="surface-foot" aria-hidden="true">
          <span>↑↓ choose</span>
          <span>Enter open</span>
          <span>Alt+Enter new tab</span>
          <span>Esc close</span>
        </div>
      </div>
    );
  }
  return (
    <PopoverContext.Provider value={{ close: () => {} }}>
      <div ref={root} role="menu" aria-label="Page menu" className="popover menu ctx-menu menu-surface" style={{ width: menu?.width ?? WIDTH }} onPointerMove={() => hot.current >= 0 && highlight(-1)}>
        {model && (
          <>
            {model.title && (
              <div className="ctx-title truncate" title={model.title}>
                {model.title}
              </div>
            )}
            {model.navRow.length > 0 && (
              <div className="ctx-nav" role="group" aria-label="Navigation">
                {model.navRow.map((n) => (
                  <button key={n.key} type="button" role="menuitem" className={n.icon ? "ctx-nav-btn" : "ctx-nav-btn is-text"} title={n.hint ? `${n.label} (${n.hint})` : n.label} aria-label={n.label} disabled={n.disabled} onClick={() => finish(n.key)}>
                    {n.icon ? <Icon name={n.icon} size={17} /> : n.label}
                  </button>
                ))}
              </div>
            )}
            {model.sections.map((s, i) => (
              <div key={i} role="group">
                {(i > 0 || model.navRow.length > 0 || model.title) && <MenuSeparator />}
                {s.heading && <MenuHeading>{s.heading}</MenuHeading>}
                {s.entries.map((entry) => (
                  <MenuItem key={entry.key} icon={entry.icon} label={entry.label} hint={entry.hint} checked={entry.checked} disabled={entry.disabled} onSelect={() => finish(entry.key)} />
                ))}
              </div>
            ))}
          </>
        )}
      </div>
    </PopoverContext.Provider>
  );
}
