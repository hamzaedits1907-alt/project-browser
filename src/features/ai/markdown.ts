// A tiny, safe renderer model for assistant replies. It only produces plain
// data (never HTML), so React renders it as text and nothing from a model or
// a web page can inject markup.

export type Block = { type: "code"; lang: string; text: string } | { type: "para"; spans: Span[] } | { type: "list"; ordered: boolean; items: Span[][] } | { type: "heading"; spans: Span[] };

export type Span = { kind: "text" | "code" | "bold"; text: string };

export function parseSpans(line: string): Span[] {
  const out: Span[] = [];
  const re = /`([^`\n]+)`|\*\*([^*\n]+)\*\*/g;
  let last = 0;
  for (let m = re.exec(line); m; m = re.exec(line)) {
    if (m.index > last) out.push({ kind: "text", text: line.slice(last, m.index) });
    if (m[1] !== undefined) out.push({ kind: "code", text: m[1] });
    else if (m[2] !== undefined) out.push({ kind: "bold", text: m[2] });
    last = m.index + m[0].length;
  }
  if (last < line.length) out.push({ kind: "text", text: line.slice(last) });
  return out;
}

export function parseBlocks(text: string): Block[] {
  const blocks: Block[] = [];
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  let i = 0;
  while (i < lines.length) {
    const line = lines[i] ?? "";
    const fence = /^```\s*([\w+-]*)\s*$/.exec(line);
    if (fence) {
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^```\s*$/.test(lines[i] ?? "")) body.push(lines[i++] ?? "");
      i++; // closing fence (or end of a still-streaming message)
      blocks.push({ type: "code", lang: fence[1] ?? "", text: body.join("\n") });
      continue;
    }
    const heading = /^#{1,4}\s+(.*)$/.exec(line);
    if (heading) {
      blocks.push({ type: "heading", spans: parseSpans(heading[1] ?? "") });
      i++;
      continue;
    }
    const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
    const numbered = /^\s*\d+[.)]\s+(.*)$/.exec(line);
    if (bullet || numbered) {
      const ordered = !!numbered;
      const items: Span[][] = [];
      while (i < lines.length) {
        const m = (ordered ? /^\s*\d+[.)]\s+(.*)$/ : /^\s*[-*]\s+(.*)$/).exec(lines[i] ?? "");
        if (!m) break;
        items.push(parseSpans(m[1] ?? ""));
        i++;
      }
      blocks.push({ type: "list", ordered, items });
      continue;
    }
    if (line.trim() === "") {
      i++;
      continue;
    }
    const para: string[] = [];
    while (i < lines.length && (lines[i] ?? "").trim() !== "" && !/^```/.test(lines[i] ?? "") && !/^\s*([-*]|\d+[.)])\s+/.test(lines[i] ?? "") && !/^#{1,4}\s/.test(lines[i] ?? "")) {
      para.push(lines[i++] ?? "");
    }
    blocks.push({ type: "para", spans: parseSpans(para.join(" ")) });
  }
  return blocks;
}
