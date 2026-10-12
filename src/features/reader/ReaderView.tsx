import { useCallback, useEffect, useRef, useState } from "react";

import { Icon } from "../../components/ui/Icon";
import { Button, IconButton, Spinner, cx } from "../../components/ui/controls";
import { hostOf } from "../../lib/format";
import { api, errorMessage } from "../../lib/ipc";
import { useOverlay } from "../../lib/overlay";
import { readLocal, writeLocal } from "../../lib/storage";
import { activeTab, getState, toast, useStore } from "../../lib/store";
import type { ReaderArticle, ReaderBlock } from "../../lib/types";
import { closeDialog, ui } from "../../lib/ui";

type ReaderTheme = "paper" | "sepia" | "night";
interface ReaderPrefs {
  size: number;
  theme: ReaderTheme;
  wide: boolean;
  serif: boolean;
  rate: number;
}
const DEFAULT_PREFS: ReaderPrefs = { size: 19, theme: "paper", wide: false, serif: true, rate: 1 };

/** The text the Listen button reads, one entry per block (pictures are skipped). */
export function speakable(blocks: ReaderBlock[]): { index: number; text: string }[] {
  return blocks.map((b, index) => ({ index, text: (b.text ?? "").trim() })).filter((b) => b.text && blocks[b.index]?.k !== "pre");
}

export function readingMinutes(words: number): number {
  return Math.max(1, Math.round(words / 230));
}

/** Reader view (F9): the article without clutter, in the browser's own typography, with read-aloud. */
export function ReaderView() {
  const open = ui.use((s) => s.dialog === "reader");
  const ready = useOverlay(open);
  if (!ready) return null;
  return <ReaderBody />;
}

function ReaderBody() {
  const tab = useStore(activeTab);
  const [article, setArticle] = useState<ReaderArticle | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [prefs, setPrefs] = useState<ReaderPrefs>(() => ({ ...DEFAULT_PREFS, ...readLocal<Partial<ReaderPrefs>>("reader.prefs", {}) }));
  const [speaking, setSpeaking] = useState<number | null>(null);
  const [paused, setPaused] = useState(false);
  const queue = useRef<{ index: number; text: string }[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const tabId = tab?.id;

  useEffect(() => {
    if (tabId === undefined) return;
    let gone = false;
    api.readerView(tabId).then(
      (a) => !gone && setArticle(a),
      (e) => !gone && setError(errorMessage(e)),
    );
    return () => {
      gone = true;
    };
  }, [tabId]);

  const savePrefs = (patch: Partial<ReaderPrefs>) => {
    setPrefs((p) => {
      const next = { ...p, ...patch };
      writeLocal("reader.prefs", next);
      return next;
    });
  };

  const stop = useCallback(() => {
    queue.current = [];
    window.speechSynthesis?.cancel();
    setSpeaking(null);
    setPaused(false);
  }, []);
  useEffect(() => stop, [stop]);

  const listen = (from = 0) => {
    if (!article) return;
    if (!("speechSynthesis" in window)) {
      toast("Read-aloud isn't available on this computer.");
      return;
    }
    window.speechSynthesis.cancel();
    queue.current = speakable(article.blocks).filter((b) => b.index >= from);
    setPaused(false);
    const lang = article.lang;
    const rate = prefs.rate;
    // One utterance per block: long utterances are cut off by some voices, and the block being read is highlighted.
    const step = () => {
      const next = queue.current.shift();
      if (!next) {
        setSpeaking(null);
        return;
      }
      const u = new SpeechSynthesisUtterance(next.text);
      u.rate = rate;
      if (lang) u.lang = lang;
      const voice = window.speechSynthesis.getVoices().find((v) => lang && v.lang.toLowerCase().startsWith(lang.toLowerCase().slice(0, 2)));
      if (voice) u.voice = voice;
      u.onend = step;
      u.onerror = () => setSpeaking(null);
      setSpeaking(next.index);
      scrollRef.current?.querySelector(`[data-block="${next.index}"]`)?.scrollIntoView({ block: "center", behavior: "smooth" });
      window.speechSynthesis.speak(u);
    };
    step();
  };

  const togglePause = () => {
    if (paused) window.speechSynthesis.resume();
    else window.speechSynthesis.pause();
    setPaused(!paused);
  };

  const copyText = () => {
    if (!article) return;
    const text = article.blocks.map((b) => (b.k === "li" ? `• ${b.text}` : b.text ?? "")).filter(Boolean).join("\n\n");
    void navigator.clipboard.writeText(`${article.title}\n\n${text}`).then(() => toast("Article copied"));
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeDialog();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className={cx("reader", `reader-${prefs.theme}`)} role="dialog" aria-modal="true" aria-label="Reader view">
      <header className="reader-bar">
        <div className="reader-site truncate">
          <Icon name="book" size={16} />
          <span className="truncate">{article?.site || (tab ? hostOf(tab.url) : "")}</span>
          {article && <span className="dim">· {readingMinutes(article.words)} min read</span>}
        </div>
        <div className="reader-tools">
          {speaking === null ? (
            <Button size="sm" variant="primary" icon="headphones" disabled={!article} onClick={() => listen(0)}>
              Listen
            </Button>
          ) : (
            <>
              <IconButton icon={paused ? "play" : "pause"} label={paused ? "Resume" : "Pause"} onClick={togglePause} />
              <IconButton icon="stop" label="Stop reading" onClick={stop} />
            </>
          )}
          <select className="reader-rate" aria-label="Reading speed" value={prefs.rate} onChange={(e) => savePrefs({ rate: Number(e.target.value) })}>
            {[0.8, 1, 1.2, 1.5, 1.8].map((r) => (
              <option key={r} value={r}>
                {r}×
              </option>
            ))}
          </select>
          <span className="reader-sep" />
          <IconButton icon="minus" label="Smaller text" onClick={() => savePrefs({ size: Math.max(14, prefs.size - 1) })} />
          <IconButton icon="plus" label="Larger text" onClick={() => savePrefs({ size: Math.min(30, prefs.size + 1) })} />
          <button type="button" className={cx("reader-font", prefs.serif && "is-serif")} title="Serif or sans-serif" onClick={() => savePrefs({ serif: !prefs.serif })}>
            Aa
          </button>
          {(["paper", "sepia", "night"] as ReaderTheme[]).map((t) => (
            <button key={t} type="button" className={cx("reader-swatch", `is-${t}`, prefs.theme === t && "is-on")} title={`${t[0]?.toUpperCase()}${t.slice(1)} theme`} aria-pressed={prefs.theme === t} onClick={() => savePrefs({ theme: t })} />
          ))}
          <IconButton icon={prefs.wide ? "restore" : "maximize"} label={prefs.wide ? "Narrow column" : "Wide column"} onClick={() => savePrefs({ wide: !prefs.wide })} />
          <IconButton icon="copy" label="Copy the article" disabled={!article} onClick={copyText} />
          <span className="reader-sep" />
          <IconButton icon="close" label="Close reader (Esc)" onClick={closeDialog} />
        </div>
      </header>
      <div className="reader-scroll" ref={scrollRef}>
        {!article && !error && (
          <div className="reader-state">
            <Spinner size={22} />
          </div>
        )}
        {error && (
          <div className="reader-state">
            <Icon name="book" size={30} />
            <p>{error}</p>
            <Button size="sm" onClick={closeDialog}>
              Back to the page
            </Button>
          </div>
        )}
        {article && (
          <article className={cx("reader-article", prefs.wide && "is-wide", prefs.serif && "is-serif")} style={{ fontSize: prefs.size }} lang={article.lang || undefined}>
            <h1 className="reader-title">{article.title}</h1>
            {article.byline && <p className="reader-byline">{article.byline}</p>}
            {article.blocks.map((b, i) => (
              <Block key={i} block={b} index={i} active={speaking === i} onListen={() => listen(i)} skipFirstH1={i === 0 && b.text === article.title} />
            ))}
            {article.truncated && <p className="reader-note">This article is long; the reader shows its first part.</p>}
            <p className="reader-note">
              Reader view shows the page's text in the browser's own layout. <button type="button" className="link" onClick={() => void navigator.clipboard.writeText(getState().browser.tabs.find((t) => t.id === tabId)?.url ?? "")}>Copy the original link</button>
            </p>
          </article>
        )}
      </div>
    </div>
  );
}

function Block({ block, index, active, onListen, skipFirstH1 }: { block: ReaderBlock; index: number; active: boolean; onListen: () => void; skipFirstH1: boolean }) {
  const props = { "data-block": index, className: cx(active && "is-speaking"), onDoubleClick: onListen, title: "Double-click to read aloud from here" };
  switch (block.k) {
    case "h1":
      return skipFirstH1 ? null : <h2 {...props}>{block.text}</h2>;
    case "h2":
      return <h2 {...props}>{block.text}</h2>;
    case "h3":
      return <h3 {...props}>{block.text}</h3>;
    case "li":
      return (
        <p {...props} className={cx("reader-li", active && "is-speaking")}>
          {block.text}
        </p>
      );
    case "quote":
      return <blockquote {...props}>{block.text}</blockquote>;
    case "pre":
      return <pre data-block={index}>{block.text}</pre>;
    case "img":
      return block.src && /^https?:/.test(block.src) ? (
        <figure data-block={index}>
          <img src={block.src} alt={block.alt ?? ""} loading="lazy" referrerPolicy="no-referrer" />
        </figure>
      ) : null;
    default:
      return <p {...props}>{block.text}</p>;
  }
}
