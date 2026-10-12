// Reads the readable text of the page for the AI assistant. Runs in the page's top frame via ExecuteScript and
// returns { text, selection, truncated }. It only reads: it never changes the page, and it never reads what the
// user typed into fields (input values, textarea contents and select options are skipped).
//
// Boilerplate (navigation, footers, sidebars, scripts, hidden elements) is left out so a request spends its
// budget on the content. The main content is the first <article> / <main> / [role=main] with enough text;
// otherwise the whole body is used.
(() => {
  const MAX_TEXT = 200000;
  const MAX_SELECTION = 20000;
  const MAX_NODES = 60000;
  const SKIP_TAGS = new Set([
    'SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE', 'SVG', 'CANVAS', 'IFRAME', 'OBJECT', 'EMBED', 'VIDEO', 'AUDIO',
    'NAV', 'FOOTER', 'ASIDE', 'TEXTAREA', 'SELECT', 'INPUT', 'OPTION', 'DATALIST', 'DIALOG',
  ]);
  const SKIP_ROLES = new Set(['navigation', 'banner', 'complementary', 'contentinfo', 'search', 'dialog', 'alertdialog', 'menu', 'menubar']);
  const BLOCK = new Set([
    'ADDRESS', 'ARTICLE', 'BLOCKQUOTE', 'BR', 'DD', 'DIV', 'DL', 'DT', 'FIELDSET', 'FIGCAPTION', 'FIGURE', 'H1', 'H2', 'H3',
    'H4', 'H5', 'H6', 'HEADER', 'HR', 'LI', 'MAIN', 'OL', 'P', 'PRE', 'SECTION', 'TABLE', 'TR', 'UL',
  ]);

  const state = { length: 0, nodes: 0, truncated: false };

  function hidden(el) {
    if (el.hidden || el.getAttribute('aria-hidden') === 'true') return true;
    const style = window.getComputedStyle(el);
    return style.display === 'none' || style.visibility === 'hidden';
  }

  function skip(el) {
    if (SKIP_TAGS.has(el.tagName)) return true;
    const role = el.getAttribute('role');
    if (role && SKIP_ROLES.has(role)) return true;
    return hidden(el);
  }

  function walk(node, out) {
    if (state.truncated) return;
    if (node.nodeType === 3) {
      const t = node.nodeValue.replace(/\s+/g, ' ');
      if (t.trim()) {
        out.push(t);
        state.length += t.length;
        if (state.length > MAX_TEXT) state.truncated = true;
      }
      return;
    }
    if (node.nodeType !== 1) return;
    if (++state.nodes > MAX_NODES) {
      state.truncated = true;
      return;
    }
    if (skip(node)) return;
    const block = BLOCK.has(node.tagName);
    if (block) out.push('\n');
    for (let child = node.firstChild; child; child = child.nextSibling) walk(child, out);
    if (block) out.push('\n');
    else if (node.tagName === 'TD' || node.tagName === 'TH') out.push(' | ');
  }

  function extract(root) {
    state.length = 0;
    state.nodes = 0;
    state.truncated = false;
    const out = [];
    walk(root, out);
    return out
      .join('')
      .split('\n')
      .map((line) => line.replace(/[ \t]+/g, ' ').trim())
      .join('\n')
      .replace(/\n{3,}/g, '\n\n')
      .trim();
  }

  let text = '';
  let truncated = false;
  try {
    const body = document.body;
    if (body) {
      const candidates = document.querySelectorAll('article, main, [role="main"]');
      for (const candidate of candidates) {
        const t = extract(candidate);
        if (t.length >= 400) {
          text = t;
          truncated = state.truncated;
          break;
        }
      }
      if (!text) {
        text = extract(body);
        truncated = state.truncated;
      }
      if (!text) text = (body.innerText || '').trim();
    }
  } catch (e) {
    text = document.body ? (document.body.innerText || '') : '';
  }
  if (text.length > MAX_TEXT) {
    text = text.slice(0, MAX_TEXT);
    truncated = true;
  }
  const selection = String(window.getSelection ? window.getSelection() : '').slice(0, MAX_SELECTION);
  return { text, selection, truncated };
})()
