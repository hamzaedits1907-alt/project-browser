// Project Browser's highlighter: marks the saved highlights of this page with the CSS Custom Highlight API (no change
// to the page's own elements) and returns how many were found. DATA is replaced by a JSON array of strings.
((texts) => {
  if (!window.CSS || !CSS.highlights || typeof Highlight === "undefined") return -1;
  const norm = (s) => s.replace(/\s+/g, " ").trim().toLowerCase();
  const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => {
      const p = n.parentElement;
      if (!p || /^(SCRIPT|STYLE|NOSCRIPT|TEXTAREA)$/.test(p.tagName)) return NodeFilter.FILTER_REJECT;
      return n.nodeValue && n.nodeValue.trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
    },
  });
  // The page's text with whitespace collapsed, and for every character where it came from.
  let flat = "";
  const at = [];
  let lastSpace = true;
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const v = n.nodeValue;
    for (let i = 0; i < v.length; i++) {
      const space = /\s/.test(v[i]);
      if (space && lastSpace) continue;
      flat += space ? " " : v[i].toLowerCase();
      at.push([n, i]);
      lastSpace = space;
    }
  }
  const ranges = [];
  for (const t of texts) {
    const q = norm(t);
    if (!q) continue;
    const i = flat.indexOf(q);
    if (i < 0) continue;
    const a = at[i];
    const b = at[i + q.length - 1];
    const r = document.createRange();
    r.setStart(a[0], a[1]);
    r.setEnd(b[0], b[1] + 1);
    ranges.push(r);
  }
  CSS.highlights.set("pb-highlight", new Highlight(...ranges));
  if (!document.getElementById("pb-highlight-style")) {
    const s = document.createElement("style");
    s.id = "pb-highlight-style";
    s.textContent = "::highlight(pb-highlight){background-color:rgba(255,214,10,.55);color:inherit}";
    (document.head || document.documentElement).appendChild(s);
  }
  if (ranges.length && window.__pbScrollTo) {
    ranges[ranges.length - 1].startContainer.parentElement?.scrollIntoView({ block: "center", behavior: "smooth" });
  }
  return ranges.length;
})(DATA)
