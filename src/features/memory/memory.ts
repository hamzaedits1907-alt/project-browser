// Turns things the user chose to keep into Project Memory records. Nothing here saves anything: the caller does
// that, from a button the user pressed.

import type { MemoryDraft, MemoryItem, MemorySource } from "../../lib/types";
import type { ClaimBattleResult, ComparisonResult, ResearchResult, Statement } from "../research/provenance";
import type { ResearchSource } from "../research/sources";

export const MEMORY_BODY_LIMIT = 50_000;
export const MEMORY_KIND_LABEL: Record<MemoryItem["kind"], string> = {
  note: "Note",
  page: "Page summary",
  research: "Research",
  session: "Session",
};

function clip(text: string): string {
  if (text.length <= MEMORY_BODY_LIMIT) return text;
  return `${text.slice(0, MEMORY_BODY_LIMIT - 60)}\n\n[Cut because it was too long to keep in one record.]`;
}

function statementLine(s: Statement): string {
  const cites = s.citations.map((c) => `${c.sourceId}${c.verified ? "" : " (quote not verified)"}`).join(", ");
  const quote = s.citations.find((c) => c.verified && c.quote)?.quote;
  const basis = s.basis === "inferred" ? " [inference]" : "";
  return `- ${s.text}${basis}${cites ? ` [${cites}]` : ""}${quote ? `\n    “${quote}”` : ""}`;
}

function section(title: string, lines: string[]): string {
  return lines.length > 0 ? `${title}\n${lines.join("\n")}\n` : "";
}

function comparisonBody(r: ComparisonResult): string {
  const table = r.table ? `${r.table.columns.join(" | ")}\n${r.table.rows.map((row) => row.join(" | ")).join("\n")}` : "";
  return [
    r.summary,
    section("Claims", r.claims.map(statementLine)),
    section("Agreements", r.agreements.map(statementLine)),
    section(
      "Contradictions",
      r.contradictions.flatMap((c) => [`${c.topic}:`, ...c.positions.map(statementLine)]),
    ),
    table && `Table\n${table}\n`,
    section("Missing", r.missing.map((m) => `- ${m}`)),
    section("Open questions", r.openQuestions.map((m) => `- ${m}`)),
  ]
    .filter(Boolean)
    .join("\n");
}

const LEANING_TEXT: Record<ClaimBattleResult["leaning"], string> = {
  supports: "The checked evidence leans towards the claim",
  contradicts: "The checked evidence leans against the claim",
  mixed: "The checked evidence points both ways",
  insufficient: "There is no checked evidence either way",
};

function claimBody(r: ClaimBattleResult): string {
  return [
    `${LEANING_TEXT[r.leaning]} (not proof).`,
    r.summary,
    section("Supporting evidence", r.supporting.map(statementLine)),
    section("Contradicting evidence", r.contradicting.map(statementLine)),
    section("Uncertain or unverified", r.uncertain.map(statementLine)),
    section("Missing information", r.missing.map((m) => `- ${m}`)),
    section("Source reliability", r.reliability.map((x) => `- ${x.sourceId}: ${x.note}`)),
    section("Caveats", r.caveats.map((c) => `- ${c}`)),
  ]
    .filter(Boolean)
    .join("\n");
}

function sourceList(sources: readonly ResearchSource[]): string {
  return sources.map((s) => `${s.id}: ${s.title} ${s.url}`).join("\n");
}

/** A kept comparison or claim check, with the pages it used. The user may edit the title and text first. */
export function draftFromResearch(result: ResearchResult, sources: readonly ResearchSource[], title?: string): MemoryDraft {
  const heading = title?.trim() || (result.mode === "claim" ? result.question : "Page comparison");
  const body = `${result.mode === "claim" ? `Question: ${result.question}\n\n${claimBody(result)}` : comparisonBody(result)}\n\nSources\n${sourceList(sources)}`;
  const memorySources: MemorySource[] = sources.map((s) => ({ url: s.url, title: s.title }));
  return { kind: "research", title: heading.slice(0, 200), body: clip(body), tags: [result.mode === "claim" ? "claim check" : "comparison"], sources: memorySources, related: [] };
}

/** A kept AI answer about one page (a summary, extracted data). */
export function draftFromAnswer(title: string, answer: string, page?: { url: string; title: string }): MemoryDraft {
  return {
    kind: "page",
    title: title.trim().slice(0, 200) || page?.title || "Saved answer",
    body: clip(answer.trim()),
    tags: ["ai"],
    sources: page ? [{ url: page.url, title: page.title }] : [],
    related: [],
  };
}

/** Splits "a, b  c" style tag input into tags. */
export function parseTags(input: string): string[] {
  return [...new Set(input.split(/[,\n]/).map((t) => t.trim().replace(/^#/, "").toLowerCase()).filter(Boolean))].slice(0, 20);
}

export function formatTags(tags: readonly string[]): string {
  return tags.join(", ");
}
