import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { Icon } from "../../components/ui/Icon";
import { MenuHeading, MenuItem, MenuSeparator, PopoverContext } from "../../components/ui/Popover";
import { api } from "../../lib/ipc";
import { applyCachedLook } from "../../lib/theme";
import { MENU_PICK, MENU_SHOW } from "./surface";
import type { MenuPick, MenuShow } from "./surface";

const WIDTH = 288;

function items(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled), [role="menuitemcheckbox"]:not(:disabled)')];
}

/**
 * The right-click menu's own window. It renders the model a shell sent, sizes and shows itself at the click (the core
 * places it), and reports the pick back to that shell. It never runs an action itself and holds no browser state.
 */
export function MenuSurface() {
  const [menu, setMenu] = useState<MenuShow | null>(null);
  const current = useRef<MenuShow | null>(null);
  const root = useRef<HTMLDivElement>(null);

  // `refocus`: give focus back to the browser window (not when the menu closed because focus went elsewhere).
  const finish = useCallback((key: string | null, refocus = true) => {
    const m = current.current;
    if (!m) return;
    current.current = null;
    void api.menuDismiss(refocus).catch(() => {});
    const pick: MenuPick = { requestId: m.requestId, key };
    void emitTo(m.owner, MENU_PICK, pick).catch(() => {});
    setMenu(null);
  }, []);

  useEffect(() => {
    document.documentElement.classList.add("is-menu-surface");
    const unlisten: Array<() => void> = [];
    let disposed = false;
    const keep = (u: () => void) => (disposed ? u() : unlisten.push(u));
    void listen<MenuShow>(MENU_SHOW, (e) => {
      // A newer menu replaces one still open: the old one is answered as dismissed.
      if (current.current && current.current.requestId !== e.payload.requestId) finish(null, false);
      applyCachedLook();
      current.current = e.payload;
      setMenu(e.payload);
    }).then(keep);
    // Clicking anywhere else (the page, another window) takes focus away: the menu closes, like any menu.
    void getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        // WebView2 hands focus back to the page right after a right-click; only a real dismissal closes the menu.
        if (!focused && current.current) {
          void api
            .menuFocusLost()
            .catch(() => true)
            .then((close) => {
              if (close) finish(null, false);
            });
        }
      })
      .then(keep);
    const block = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", block);
    void api.menuSurfaceReady().catch(() => {});
    return () => {
      disposed = true;
      unlisten.forEach((u) => u());
      document.removeEventListener("contextmenu", block);
    };
  }, [finish]);

  // Rendered: measure and show at the click. The window has the exact size of the menu.
  useLayoutEffect(() => {
    const el = root.current;
    if (!menu || !el) return;
    const r = el.getBoundingClientRect();
    void api
      .menuPresent(Math.ceil(r.width), Math.ceil(r.height))
      .then(() => el.focus({ preventScroll: true }))
      .catch(() => finish(null));
  }, [menu, finish]);

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const el = root.current;
    if (!el) return;
    if (e.key === "Escape") {
      e.preventDefault();
      finish(null);
      return;
    }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
    const list = items(el);
    if (list.length === 0) return;
    e.preventDefault();
    const at = list.indexOf(document.activeElement as HTMLElement);
    const next = e.key === "Home" ? 0 : e.key === "End" ? list.length - 1 : e.key === "ArrowDown" ? (at + 1) % list.length : at <= 0 ? list.length - 1 : at - 1;
    list[next]?.focus();
  };

  const model = menu?.model;
  return (
    <PopoverContext.Provider value={{ close: () => {} }}>
      <div ref={root} role="menu" aria-label="Page menu" tabIndex={-1} className="popover menu ctx-menu menu-surface" style={{ width: WIDTH }} onKeyDown={onKeyDown}>
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
                  <button key={n.key} type="button" role="menuitem" className="ctx-nav-btn" title={n.label} aria-label={n.label} onClick={() => finish(n.key)}>
                    {n.icon && <Icon name={n.icon} size={17} />}
                  </button>
                ))}
              </div>
            )}
            {model.sections.map((s, i) => (
              <div key={i} role="group">
                {(i > 0 || model.navRow.length > 0 || model.title) && <MenuSeparator />}
                {s.heading && <MenuHeading>{s.heading}</MenuHeading>}
                {s.entries.map((entry) => (
                  <MenuItem key={entry.key} icon={entry.icon} label={entry.label} hint={entry.hint} checked={entry.checked} onSelect={() => finish(entry.key)} />
                ))}
              </div>
            ))}
          </>
        )}
      </div>
    </PopoverContext.Provider>
  );
}
