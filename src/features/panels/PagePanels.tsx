// Side panels about pages: the Highlighter's saved highlights, and every picture, video and sound on the current page.

import { useMemo, useState, useSyncExternalStore } from "react";

import { Favicon } from "../../components/ui/Favicon";
import { Icon } from "../../components/ui/Icon";
import { EmptyState } from "../../components/ui/Section";
import { Button, IconButton, Spinner, cx } from "../../components/ui/controls";
import { hostOf } from "../../lib/format";
import { api } from "../../lib/ipc";
import { activeTab, attempt, toast, useStore } from "../../lib/store";
import type { PageMediaItem } from "../../lib/types";
import { useLoad } from "../../lib/useLoad";
import { isInternalUrl } from "../../lib/urls";
import { allHighlights, linkToHighlight, pageKey, removeHighlight, subscribeHighlights } from "../highlights/highlights";

// ---- Highlights ----------------------------------------------------------------------------------

let snapshot = allHighlights();
let version = 0;
const useHighlightStore = () =>
  useSyncExternalStore(
    (l) =>
      subscribeHighlights(() => {
        snapshot = allHighlights();
        version++;
        l();
      }),
    () => version,
  );

/** Every page with highlights, newest first; the current page on top. Click a highlight to open the page at it. */
export function HighlightsPanel() {
  useHighlightStore();
  const tab = useStore(activeTab);
  const [query, setQuery] = useState("");
  const current = tab ? pageKey(tab.url) : null;
  const pages = useMemo(() => {
    const q = query.trim().toLowerCase();
    return Object.entries(snapshot)
      .map(([key, p]) => ({ key, ...p, texts: q ? p.texts.filter((t) => t.text.toLowerCase().includes(q) || p.title.toLowerCase().includes(q)) : p.texts }))
      .filter((p) => p.texts.length > 0)
      .sort((a, b) => (a.key === current ? -1 : b.key === current ? 1 : Math.max(...b.texts.map((t) => t.at)) - Math.max(...a.texts.map((t) => t.at))));
    // `version` changes with every edit (useHighlightStore).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, current, version]);

  if (Object.keys(snapshot).length === 0) {
    return (
      <div className="panel-body">
        <EmptyState icon="edit" title="No highlights yet">
          Select text on a page, right-click it and choose “Highlight”. Highlights come back every time you open the page.
        </EmptyState>
      </div>
    );
  }
  return (
    <div className="panel-body">
      <input className="input panel-search" type="search" placeholder="Search highlights" aria-label="Search highlights" value={query} onChange={(e) => setQuery(e.target.value)} />
      {pages.map((p) => (
        <section key={p.key} className={cx("hl-page", p.key === current && "is-current")}>
          <button type="button" className="panel-link hl-title" title={p.url} onClick={() => void attempt(() => api.navigate(p.url))}>
            <Favicon src={null} fallback={(hostOf(p.url).charAt(0) || "•").toUpperCase()} seed={hostOf(p.url)} size={16} />
            <span className="panel-text">
              <span className="truncate">{p.title || p.url}</span>
              <span className="panel-sub truncate">{hostOf(p.url)}</span>
            </span>
          </button>
          {p.texts.map((t) => (
            <div key={t.text} className="hl-item">
              <button type="button" className="hl-text" title="Open the page at this highlight" onClick={() => void attempt(() => api.navigate(linkToHighlight(p.url, t.text)))}>
                {t.text.length > 280 ? `${t.text.slice(0, 279)}…` : t.text}
              </button>
              <div className="hl-actions">
                <IconButton icon="copy" label="Copy" size={13} onClick={() => void navigator.clipboard.writeText(t.text).then(() => toast("Highlight copied"))} />
                <IconButton icon="link" label="Copy link to this highlight" size={13} onClick={() => void navigator.clipboard.writeText(linkToHighlight(p.url, t.text)).then(() => toast("Link copied"))} />
                <IconButton icon="trash" label="Remove highlight" size={13} onClick={() => removeHighlight(p.key, t.text)} />
              </div>
            </div>
          ))}
        </section>
      ))}
    </div>
  );
}

// ---- Media on this page --------------------------------------------------------------------------

type MediaFilter = "all" | "image" | "video" | "audio";

/** Every picture, video and sound on the current page, with Save (into the download folder) and Open. */
export function PageMediaPanel() {
  const tab = useStore(activeTab);
  const web = !!tab && !isInternalUrl(tab.url) && tab.lifecycle === "live";
  const key = web && tab ? `${tab.id}|${tab.url}|${tab.loading}` : null;
  const media = useLoad(() => (web && tab ? api.pageMedia(tab.id) : Promise.resolve([] as PageMediaItem[])), key);
  const [filter, setFilter] = useState<MediaFilter>("all");
  const [saving, setSaving] = useState<Set<string>>(new Set());
  const items = (media.data ?? []).filter((m) => filter === "all" || m.kind === filter);

  const save = async (m: PageMediaItem) => {
    setSaving((s) => new Set(s).add(m.url));
    const path = await attempt(() => api.saveUrl(m.url, tab?.url), "Could not save it");
    setSaving((s) => {
      const n = new Set(s);
      n.delete(m.url);
      return n;
    });
    if (path) toast(`Saved to ${path}`);
  };
  const saveAll = async () => {
    let n = 0;
    for (const m of items.slice(0, 100)) {
      const path = await api.saveUrl(m.url, tab?.url).catch(() => null);
      if (path) n++;
    }
    toast(`Saved ${n} of ${Math.min(items.length, 100)} files to your download folder`);
  };

  if (!web) {
    return (
      <div className="panel-body">
        <EmptyState icon="image" title="Open a web page">
          Pictures, videos and sounds on the page you're looking at appear here, ready to save.
        </EmptyState>
      </div>
    );
  }
  const counts = { image: 0, video: 0, audio: 0 };
  for (const m of media.data ?? []) counts[m.kind]++;
  return (
    <div className="panel-body">
      <div className="panel-tabs" role="tablist">
        {(["all", "image", "video", "audio"] as MediaFilter[]).map((f) => (
          <button key={f} type="button" role="tab" aria-selected={filter === f} className={cx("panel-tab", filter === f && "is-on")} onClick={() => setFilter(f)}>
            {f === "all" ? `All ${media.data?.length ?? 0}` : f === "image" ? `Pictures ${counts.image}` : f === "video" ? `Videos ${counts.video}` : `Sounds ${counts.audio}`}
          </button>
        ))}
      </div>
      <div className="media-tools">
        <Button size="sm" icon="refresh" onClick={() => media.reload()}>
          Look again
        </Button>
        <Button size="sm" icon="download" disabled={items.length === 0} onClick={() => void saveAll()}>
          Save all{items.length > 100 ? " (first 100)" : ""}
        </Button>
      </div>
      {!media.data && <Spinner />}
      {media.data && items.length === 0 && (
        <EmptyState icon="image" title="Nothing found">
          Videos that stream in pieces (most video sites) can't be saved as one file.
        </EmptyState>
      )}
      <div className="media-grid">
        {items.map((m) => (
          <div key={m.url} className="media-cell" title={m.alt || m.url}>
            <div className="media-thumb">
              {m.kind === "image" ? <img src={m.url} alt={m.alt} loading="lazy" referrerPolicy="no-referrer" /> : <Icon name={m.kind === "video" ? "play" : "headphones"} size={22} />}
            </div>
            <div className="media-meta">
              <span className="truncate">{m.width && m.height ? `${m.width}×${m.height}` : m.kind}</span>
              <IconButton icon="external" label="Open in a new tab" size={13} onClick={() => void attempt(() => api.tabNew(m.url, tab?.private))} />
              <IconButton icon={saving.has(m.url) ? "hourglass" : "download"} label="Save" size={13} disabled={saving.has(m.url)} onClick={() => void save(m)} />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
