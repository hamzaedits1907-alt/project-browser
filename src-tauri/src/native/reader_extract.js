// Reader view: finds the article on the page and returns it as plain blocks for the browser's own reader.
// Runs in the page's top frame via ExecuteScript. It only reads: it never changes the page and never reads what the
// user typed into fields. Returns { title, byline, site, lang, blocks: [{ k, text?, src?, alt? }], words, truncated }.
// k is one of h1 h2 h3 p li quote pre img. Text is plain text; the shell renders it as text (never as HTML).
(() => {
  const MAX_BLOCKS = 1500;
  const MAX_CHARS = 400000;
  const SKIP = new Set(['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE', 'SVG', 'CANVAS', 'IFRAME', 'OBJECT', 'EMBED', 'NAV',
    'FOOTER', 'ASIDE', 'FORM', 'BUTTON', 'INPUT', 'TEXTAREA', 'SELECT', 'DIALOG', 'HEADER', 'TABLE']);
  const NOISE = /(^|[\s_-])(comment|share|social|related|promo|advert|sponsor|newsletter|subscribe|sidebar|footer|cookie|banner|popup|modal|breadcrumb|toolbar|menu|nav|infobox|navbox|metadata|toc|reflist|references|hatnote|mw-editsection|catlinks|ambox)([\s_-]|$)/i;

  const hidden = (el) => {
    if (el.hidden || el.getAttribute('aria-hidden') === 'true') return true;
    const s = getComputedStyle(el);
    return s.display === 'none' || s.visibility === 'hidden';
  };
  // Citation markers ([1], [edit]) are noise when read on their own or aloud.
  const textOf = (el) => (el.innerText || el.textContent || '').replace(/\[(\d+|edit|citation needed)\]/gi, '').replace(/\s+/g, ' ').trim();

  // Score candidate containers by the amount of paragraph text directly inside them (a light Readability).
  const score = new Map();
  for (const p of document.querySelectorAll('p, pre, blockquote, li')) {
    const len = textOf(p).length;
    if (len < 40) continue;
    const parent = p.parentElement;
    if (!parent) continue;
    const pts = 1 + Math.min(len / 100, 3) + (textOf(p).split(',').length - 1) * 0.3;
    score.set(parent, (score.get(parent) || 0) + pts);
    if (parent.parentElement) score.set(parent.parentElement, (score.get(parent.parentElement) || 0) + pts / 2);
  }
  let root = document.querySelector('article') || document.querySelector('main, [role=main]');
  let best = null;
  let bestScore = 0;
  for (const [el, s] of score) {
    const id = (el.className || '') + ' ' + (el.id || '');
    const adjusted = typeof id === 'string' && NOISE.test(id) ? s * 0.3 : s;
    if (adjusted > bestScore) {
      best = el;
      bestScore = adjusted;
    }
  }
  if (!root || (best && root.contains(best) === false && bestScore > 8)) root = best || document.body;
  if (!root) return { title: document.title, blocks: [], words: 0, truncated: false };

  const blocks = [];
  let chars = 0;
  let truncated = false;
  const push = (b) => {
    if (blocks.length >= MAX_BLOCKS || chars > MAX_CHARS) {
      truncated = true;
      return;
    }
    chars += (b.text || '').length;
    blocks.push(b);
  };
  const walk = (el) => {
    if (truncated) return;
    for (let n = el.firstElementChild; n; n = n.nextElementSibling) {
      if (SKIP.has(n.tagName) || hidden(n)) continue;
      const cls = (typeof n.className === 'string' ? n.className : '') + ' ' + (n.id || '');
      if (NOISE.test(cls) && textOf(n).length < 600) continue;
      const tag = n.tagName;
      if (/^H[1-6]$/.test(tag)) {
        const t = textOf(n);
        if (t) push({ k: tag === 'H1' ? 'h1' : tag === 'H2' ? 'h2' : 'h3', text: t });
      } else if (tag === 'P') {
        const t = textOf(n);
        if (t.length > 1) push({ k: 'p', text: t });
        for (const img of n.querySelectorAll('img')) addImg(img);
      } else if (tag === 'PRE') {
        push({ k: 'pre', text: (n.innerText || n.textContent || '').slice(0, 20000) });
      } else if (tag === 'BLOCKQUOTE') {
        const t = textOf(n);
        if (t) push({ k: 'quote', text: t });
      } else if (tag === 'UL' || tag === 'OL') {
        for (const li of n.querySelectorAll(':scope > li')) {
          const t = textOf(li);
          if (t) push({ k: 'li', text: t });
        }
      } else if (tag === 'IMG') {
        addImg(n);
      } else if (tag === 'FIGURE') {
        const img = n.querySelector('img');
        if (img) addImg(img);
        const cap = n.querySelector('figcaption');
        if (cap && textOf(cap)) push({ k: 'p', text: textOf(cap) });
      } else {
        walk(n);
      }
    }
  };
  const seenImg = new Set();
  const addImg = (img) => {
    const src = img.currentSrc || img.src || '';
    if (!/^https?:/.test(src) || seenImg.has(src)) return;
    if ((img.naturalWidth || img.width || 0) < 120) return;
    seenImg.add(src);
    push({ k: 'img', src, alt: (img.alt || '').slice(0, 300) });
  };
  walk(root);

  const meta = (sel) => (document.querySelector(sel) || {}).content || '';
  const words = blocks.reduce((n, b) => n + (b.text ? b.text.split(/\s+/).length : 0), 0);
  return {
    title: meta('meta[property="og:title"]') || document.title || '',
    byline: meta('meta[name="author"]') || textOf(document.querySelector('[rel=author], .author, .byline') || document.createElement('i')).slice(0, 120),
    site: meta('meta[property="og:site_name"]') || location.hostname,
    lang: document.documentElement.lang || '',
    blocks,
    words,
    truncated,
  };
})();
