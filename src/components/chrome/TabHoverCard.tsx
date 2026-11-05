// Tab hover cards, like Chrome's: resting the pointer on a tab shows its title, site, state and a small picture of the
// page. The card is drawn over the live page through a hole (lib/overlay.ts), never takes focus and never hides the
// page. Pictures come from the core (native/thumbs.rs): a few kilobytes each.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { RefObject } from "react";

import { formatBytes, hostOf } from "../../lib/format";
import { api } from "../../lib/ipc";
import { newHoleId, setHole } from "../../lib/overlay";
import { getState } from "../../lib/store";
import type { Tab } from "../../lib/types";
import { isInternalUrl, tabTitle } from "../../lib/urls";
import { Icon } from "../ui/Icon";

const SHOW_AFTER_MS = 550;
const WIDTH = 280;

export function TabHoverCard({ strip, vertical, suppressed }: { strip: RefObject<HTMLElement | null>; vertical: boolean; suppressed: boolean }) {
  const [shown, setShown] = useState<{ tab: Tab; rect: DOMRect } | null>(null);
  const [picture, setPicture] = useState<string | null>(null);
  const timer = useRef(0);
  const [holeId] = useState(newHoleId);
  const cardRef = useRef<HTMLDivElement>(null);
  const shownRef = useRef(false);

  // Which tab the pointer rests on (pointer events on the strip, so tabs themselves need no handlers).
  useEffect(() => {
    const el = strip.current;
    if (!el) return;
    let current: number | null = null;
    const over = (e: PointerEvent) => {
      const tabEl = (e.target as HTMLElement).closest<HTMLElement>("[data-tab-id]");
      const id = tabEl ? Number(tabEl.dataset.tabId) : null;
      if (id === current) return;
      current = id;
      window.clearTimeout(timer.current);
      if (id === null || !tabEl) {
        setShown(null);
        return;
      }
      // Moving from one tab to the next while a card shows switches at once, as in Chrome.
      const delay = shownRef.current ? 60 : SHOW_AFTER_MS;
      timer.current = window.setTimeout(() => {
        const tab = getState().browser.tabs.find((t) => t.id === id);
        if (tab) setShown({ tab, rect: tabEl.getBoundingClientRect() });
      }, delay);
    };
    const leave = () => {
      current = null;
      window.clearTimeout(timer.current);
      setShown(null);
    };
    const down = () => {
      window.clearTimeout(timer.current);
      setShown(null);
    };
    el.addEventListener("pointerover", over);
    el.addEventListener("pointerleave", leave);
    el.addEventListener("pointerdown", down, true);
    return () => {
      window.clearTimeout(timer.current);
      el.removeEventListener("pointerover", over);
      el.removeEventListener("pointerleave", leave);
      el.removeEventListener("pointerdown", down, true);
    };
  }, [strip]);
  useEffect(() => {
    shownRef.current = !!shown;
  }, [shown]);

  // The page picture (web pages only).
  const tabId = shown?.tab.id ?? null;
  const web = !!shown && !isInternalUrl(shown.tab.url);
  useEffect(() => {
    if (tabId === null || !web) return;
    let live = true;
    let url: string | null = null;
    void api
      .tabThumbnail(tabId)
      .then((u) => {
        url = u;
        if (live) setPicture(u);
        else if (u) URL.revokeObjectURL(u);
      })
      .catch(() => {});
    return () => {
      live = false;
      setPicture(null);
      if (url) URL.revokeObjectURL(url);
    };
  }, [tabId, web]);

  const visible = shown && !suppressed;
  const left = shown ? (vertical ? shown.rect.right + 8 : Math.max(8, Math.min(shown.rect.left, window.innerWidth - WIDTH - 8))) : 0;
  const top = shown ? (vertical ? shown.rect.top : shown.rect.bottom + 6) : 0;

  useLayoutEffect(() => {
    const el = cardRef.current;
    if (!visible || !el) {
      setHole(holeId, null);
      return;
    }
    setHole(holeId, { x: left, y: top, width: el.offsetWidth, height: el.offsetHeight });
  });
  useEffect(() => () => setHole(holeId, null), [holeId]);

  if (!visible || !shown) return null;
  const t = shown.tab;
  const notes = [
    t.lifecycle === "sleeping" ? "Sleeping: uses no memory until you open it" : null,
    t.audible && !t.muted ? "Playing sound" : null,
    t.muted ? "Muted" : null,
    t.heavy ? "Using a lot of memory or CPU" : null,
    t.private ? "Private" : null,
  ].filter(Boolean);
  const memory = (t as Tab & { memoryBytes?: number }).memoryBytes;
  return (
    <div ref={cardRef} className="tab-card" style={{ left, top, width: WIDTH }} aria-hidden="true">
      <div className="tab-card-title">{tabTitle(t)}</div>
      <div className="tab-card-host">
        {isInternalUrl(t.url) ? "Project Browser" : hostOf(t.url)}
        {typeof memory === "number" && memory > 0 && <span> · {formatBytes(memory)}</span>}
      </div>
      {notes.map((n) => (
        <div key={n} className="tab-card-note">
          <Icon name={n?.startsWith("Sleeping") ? "moon" : n?.startsWith("Playing") ? "volume" : "info"} size={12} />
          {n}
        </div>
      ))}
      {web && picture && <img className="tab-card-pic" src={picture} alt="" />}
    </div>
  );
}
