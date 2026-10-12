// Turns the model's answer to a comparison or claim check into something the user can trust the shape of.
//
// The model is asked for JSON in which every statement names the source it came from (S1, S2, …) and quotes the
// words it relied on. Nothing the model says is believed here: a source id that wasn't supplied is dropped, and a
// quote is checked against the page text that was actually sent. A statement whose quote cannot be found is kept,
// but shown as unverified and never counted as evidence. Agreement between pages is not treated as proof.

import { hostOf } from "./sources";
import type { ResearchSource } from "./sources";

export type Basis = "stated" | "inferred";

export interface Citation {
  /** The id used in the prompt ("S1"). */
  sourceId: string;
  url: string;
  title: string;
  /** The words the model quoted, if it quoted any. */
  quote: string | null;
  /** The quote was found, word for word (ignoring case, spacing and punctuation style), in the source text sent. */
  verified: boolean;
}

export interface Statement {
  text: string;
  basis: Basis;
  citations: Citation[];
  /** True only when every citation is verified and there is at least one. */
  grounded: boolean;
  /** Why the statement is not fully grounded, in words for the user. */
  problems: string[];
}

export interface Contradiction {
  topic: string;
  positions: Statement[];
}

export interface Reliability {
  sourceId: string;
  url: string;
  title: string;
  note: string;
}

export interface TableData {
  columns: string[];
  rows: string[][];
}

export interface ComparisonResult {
  mode: "compare";
  summary: string;
  claims: Statement[];
  agreements: Statement[];
  contradictions: Contradiction[];
  missing: string[];
  openQuestions: string[];
  table: TableData | null;
  /** Statements removed because they named no source that was supplied. */
  dropped: number;
}

export type Leaning = "supports" | "contradicts" | "mixed" | "insufficient";

export interface ClaimBattleResult {
  mode: "claim";
  question: string;
  summary: string;
  supporting: Statement[];
  contradicting: Statement[];
  /** Everything not backed by a verified quote, plus what the model itself marked uncertain. */
  uncertain: Statement[];
  missing: string[];
  reliability: Reliability[];
  /** What the verified evidence points to. Never "proven": see `caveats`. */
  leaning: Leaning;
  /** Plain-language limits on the reading above. */
  caveats: string[];
  dropped: number;
}

export type ResearchResult = ComparisonResult | ClaimBattleResult;

export type ParseOutcome<T> = { ok: true; result: T } | { ok: false; error: string };

const MAX_TEXT = 700;
const MAX_QUOTE = 500;
const MIN_QUOTE = 12;
const MAX_ITEMS = 40;

// ---- reading the model's JSON --------------------------------------------------------------------

/** Finds the JSON object in a reply that may be wrapped in a code fence or surrounded by prose. */
export function extractJson(reply: string): unknown {
  const fenced = /```(?:json)?\s*([\s\S]*?)```/i.exec(reply);
  const candidates = [fenced?.[1], reply].filter((c): c is string => !!c);
  for (const candidate of candidates) {
    const start = candidate.indexOf("{");
    const end = candidate.lastIndexOf("}");
    if (start < 0 || end <= start) continue;
    try {
      return JSON.parse(candidate.slice(start, end + 1));
    } catch {
      /* try the next candidate */
    }
  }
  return null;
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function str(v: unknown, max = MAX_TEXT): string {
  return typeof v === "string" ? v.replace(/\s+/g, " ").trim().slice(0, max) : "";
}

function strings(v: unknown): string[] {
  return Array.isArray(v) ? v.map((x) => str(x)).filter(Boolean).slice(0, MAX_ITEMS) : [];
}

function records(v: unknown): Record<string, unknown>[] {
  return Array.isArray(v) ? v.filter(isRecord).slice(0, MAX_ITEMS) : [];
}

// ---- checking a quote against the page ------------------------------------------------------------

/** Lower-cases and flattens spacing, quote marks, dashes and ellipses, so harmless typography differences don't matter. */
export function normalizeForMatch(text: string): string {
  return text
    .normalize("NFKC")
    .toLowerCase()
    .replace(/[‘’‚‛′]/g, "'")
    .replace(/[“”„‟″]/g, '"')
    .replace(/[‐-―−]/g, "-")
    .replace(/…/g, "...")
    .replace(/\s+/g, " ")
    .trim();
}

/**
 * Whether `quote` appears in `sourceText`. A quote with "..." in it is checked piece by piece, in order. Quotes too
 * short to mean anything (a few words) are never accepted: they would match almost any page.
 */
export function quoteAppearsIn(quote: string, sourceText: string): boolean {
  const hay = normalizeForMatch(sourceText);
  const cleaned = normalizeForMatch(quote).replace(/^["'`]+|["'`]+$/g, "").trim();
  if (cleaned.replace(/\.\.\./g, "").trim().length < MIN_QUOTE) return false;
  const pieces = cleaned.split("...").map((p) => p.trim()).filter((p) => p.length > 0);
  let from = 0;
  for (const piece of pieces) {
    const at = hay.indexOf(piece, from);
    if (at < 0) return false;
    from = at + piece.length;
  }
  return pieces.length > 0;
}

function sourceById(sources: readonly ResearchSource[]): Map<string, ResearchSource> {
  return new Map(sources.map((s) => [s.id.toUpperCase(), s]));
}

function sourceIdsOf(v: unknown): string[] {
  const raw = Array.isArray(v) ? v : typeof v === "string" ? [v] : [];
  return raw.map((x) => str(x, 12).toUpperCase().replace(/^\[|\]$/g, "")).filter(Boolean);
}

interface Raw {
  text: string;
  basis?: unknown;
  sources: string[];
  quote: string;
  /** For items the model marked uncertain: its own reason. */
  why: string;
}

function rawStatement(r: Record<string, unknown>, textKey = "text"): Raw {
  return {
    text: str(r[textKey]) || str(r.text) || str(r.claim),
    basis: r.basis,
    sources: sourceIdsOf(r.sources ?? r.source),
    quote: str(r.quote, MAX_QUOTE),
    why: str(r.why, 300),
  };
}

/**
 * Builds a statement, checking its citations. Returns `null` when the statement names no source that was supplied
 * and is not marked as the model's own inference: such a statement could only be an uncited claim.
 */
function checkStatement(raw: Raw, sources: Map<string, ResearchSource>): Statement | null {
  if (!raw.text) return null;
  const known = [...new Set(raw.sources)].filter((id) => sources.has(id));
  const unknown = raw.sources.filter((id) => !sources.has(id));
  const inferred = raw.basis === "inferred";
  const problems: string[] = [];
  if (unknown.length > 0) problems.push(`Named ${unknown.join(", ")}, which wasn't one of the pages you selected.`);
  if (known.length === 0 && !inferred) return null;

  const citations: Citation[] = known.map((id) => {
    const source = sources.get(id) as ResearchSource;
    const quote = raw.quote || null;
    const verified = quote !== null && quoteAppearsIn(quote, source.text);
    return { sourceId: id, url: source.url, title: source.title, quote, verified };
  });
  // One quote is checked against every page it cites and has to be found in at least one of them.
  const anyVerified = citations.some((c) => c.verified);

  const basis: Basis = inferred ? "inferred" : "stated";
  if (basis === "stated") {
    if (!raw.quote) problems.push("No quote was given, so this can't be checked against the page.");
    else if (!anyVerified) problems.push("The quoted words weren't found in the page text that was read.");
  } else {
    problems.push("This is the model's own inference, not something a page says.");
  }
  if (raw.why) problems.push(raw.why);
  const grounded = basis === "stated" && citations.length > 0 && anyVerified;
  return { text: raw.text, basis, citations, grounded, problems };
}

function statements(list: Record<string, unknown>[], sources: Map<string, ResearchSource>, textKey?: string): { kept: Statement[]; dropped: number } {
  const kept: Statement[] = [];
  let dropped = 0;
  for (const r of list) {
    const s = checkStatement(rawStatement(r, textKey), sources);
    if (s) kept.push(s);
    else dropped++;
  }
  return { kept, dropped };
}

function tableOf(v: unknown): TableData | null {
  if (!isRecord(v)) return null;
  const columns = strings(v.columns).slice(0, 8);
  if (columns.length === 0) return null;
  const rows = (Array.isArray(v.rows) ? v.rows : [])
    .filter(Array.isArray)
    .slice(0, MAX_ITEMS)
    .map((row) => Array.from({ length: columns.length }, (_, i) => str((row as unknown[])[i], 300)));
  return { columns, rows };
}

// ---- Research Constellation ------------------------------------------------------------------------

export function parseComparison(reply: string, sources: readonly ResearchSource[]): ParseOutcome<ComparisonResult> {
  const json = extractJson(reply);
  if (!isRecord(json)) return { ok: false, error: "The model's answer wasn't in the structure this view needs, so nothing could be checked against your pages." };
  const byId = sourceById(sources);
  const claims = statements(records(json.claims), byId);
  const agreements = statements(records(json.agreements), byId);
  let dropped = claims.dropped + agreements.dropped;
  const contradictions: Contradiction[] = [];
  for (const c of records(json.contradictions)) {
    const positions = statements(records(c.positions), byId);
    dropped += positions.dropped;
    // A contradiction needs at least two cited positions; anything less is just one claim.
    if (positions.kept.length >= 2) contradictions.push({ topic: str(c.topic) || "Disagreement", positions: positions.kept });
    else dropped += positions.kept.length;
  }
  return {
    ok: true,
    result: {
      mode: "compare",
      summary: str(json.summary, 1200),
      claims: claims.kept,
      agreements: agreements.kept,
      contradictions,
      missing: strings(json.missing),
      openQuestions: strings(json.openQuestions),
      table: tableOf(json.table),
      dropped,
    },
  };
}

// ---- Claim Battle ----------------------------------------------------------------------------------

/** Moves a statement to "uncertain" when its evidence isn't grounded, instead of letting it count as support or opposition. */
function split(list: Statement[]): { grounded: Statement[]; ungrounded: Statement[] } {
  return { grounded: list.filter((s) => s.grounded), ungrounded: list.filter((s) => !s.grounded) };
}

/**
 * What verified evidence points to. Several pages saying the same thing is not counted as stronger than one: only
 * the number of distinct sites matters, and even then the result is a "leaning", with caveats the user can read.
 */
export function assessEvidence(supporting: readonly Statement[], contradicting: readonly Statement[]): { leaning: Leaning; caveats: string[] } {
  const hosts = (list: readonly Statement[]) => new Set(list.flatMap((s) => s.citations.filter((c) => c.verified).map((c) => hostOf(c.url))));
  const sup = hosts(supporting);
  const con = hosts(contradicting);
  const caveats = ["Pages repeating each other is not proof: check the original sources before relying on this."];
  let leaning: Leaning;
  if (sup.size === 0 && con.size === 0) {
    leaning = "insufficient";
    caveats.unshift("None of the pages gave a quote that could be checked, so there is no verified evidence either way.");
  } else if (sup.size > 0 && con.size > 0) {
    leaning = "mixed";
    caveats.unshift("The pages disagree.");
  } else if (sup.size > 0) {
    leaning = "supports";
    if (sup.size === 1) caveats.unshift("Only one site backs this up.");
  } else {
    leaning = "contradicts";
    if (con.size === 1) caveats.unshift("Only one site argues against it.");
  }
  return { leaning, caveats };
}

export function parseClaimBattle(reply: string, question: string, sources: readonly ResearchSource[]): ParseOutcome<ClaimBattleResult> {
  const json = extractJson(reply);
  if (!isRecord(json)) return { ok: false, error: "The model's answer wasn't in the structure this view needs, so nothing could be checked against your pages." };
  const byId = sourceById(sources);
  const sup = statements(records(json.supporting), byId);
  const con = statements(records(json.contradicting), byId);
  const unc = statements(records(json.uncertain), byId);
  const supSplit = split(sup.kept);
  const conSplit = split(con.kept);
  const uncertain = [...unc.kept, ...supSplit.ungrounded, ...conSplit.ungrounded];
  const { leaning, caveats } = assessEvidence(supSplit.grounded, conSplit.grounded);
  if (supSplit.ungrounded.length + conSplit.ungrounded.length > 0) {
    caveats.push("Some statements were moved to “uncertain” because their quotes couldn't be found in the pages.");
  }
  const reliability: Reliability[] = [];
  for (const r of records(json.reliability)) {
    const id = sourceIdsOf(r.source ?? r.sources)[0];
    const source = id ? byId.get(id) : undefined;
    const note = str(r.note);
    if (source && note) reliability.push({ sourceId: source.id, url: source.url, title: source.title, note });
  }
  return {
    ok: true,
    result: {
      mode: "claim",
      question,
      summary: str(json.summary, 1200),
      supporting: supSplit.grounded,
      contradicting: conSplit.grounded,
      uncertain,
      missing: strings(json.missing),
      reliability,
      leaning,
      caveats,
      dropped: sup.dropped + con.dropped + unc.dropped,
    },
  };
}
