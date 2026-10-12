// The pages a comparison or claim check reads: how they are chosen, sized and presented to the model.

import type { PageSnapshotView } from "../../lib/types";

/** One page, as the model sees it. `id` is what the model must cite. */
export interface ResearchSource {
  id: string;
  tabId: number;
  url: string;
  title: string;
  /** The text that was sent. Quotes are checked against exactly this. */
  text: string;
  /** The page has more text than was sent. */
  truncated: boolean;
  totalChars: number;
}

/** Characters of fixed instructions added to the page text; used only for the estimate shown to the user. */
export const PROMPT_OVERHEAD_CHARS = 2600;

export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "").toLowerCase();
  } catch {
    return url;
  }
}

/** Page text per source: the user's context limit shared evenly, with a floor so no page is reduced to nothing. */
export function perSourceChars(limit: number, count: number): number {
  if (count <= 0) return limit;
  return Math.max(1000, Math.floor((limit - PROMPT_OVERHEAD_CHARS) / count));
}

/** A rough token count (about four characters per token). Only ever shown as an estimate. */
export function estimateTokens(chars: number): number {
  return Math.ceil(Math.max(0, chars) / 4);
}

export function estimateRequestTokens(sources: readonly Pick<ResearchSource, "text">[]): number {
  return estimateTokens(sources.reduce((n, s) => n + s.text.length, 0) + PROMPT_OVERHEAD_CHARS);
}

/** Numbers the pages S1, S2, … in the order the user listed them. */
export function toSources(snapshots: readonly PageSnapshotView[]): ResearchSource[] {
  return snapshots.map((s, i) => ({
    id: `S${i + 1}`,
    tabId: s.tabId,
    url: s.url,
    title: s.title.trim() || hostOf(s.url),
    text: s.text,
    truncated: s.truncated,
    totalChars: s.totalChars,
  }));
}

/** A page-supplied title or address flattened to one line, so it can't add lines or fake the prompt's structure. */
export function oneLine(text: string, max = 200): string {
  return text.replace(/[\r\n\t]+/g, " ").replace(/\s+/g, " ").trim().slice(0, max);
}

/** Page text with the fence markers defanged, so a page can't write its own end-of-source marker. */
export function fenceSafe(text: string): string {
  return text.replace(/<<<|>>>/g, (m) => (m === "<<<" ? "‹‹‹" : "›››"));
}
