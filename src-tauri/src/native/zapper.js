// Element zapper: the user points at something on the page and clicks it; the browser hides it on this site from now
// on. Resolves with a CSS selector for the element, or null (Escape, or nothing chosen within two minutes).
new Promise((resolve) => {
  if (window.__pbZapping) return resolve(null);
  window.__pbZapping = true;
  const box = document.createElement("div");
  box.setAttribute("style", "position:fixed;z-index:2147483647;pointer-events:none;border:2px solid #ff4d6d;background:rgba(255,77,109,.18);border-radius:3px;transition:all .06s");
  const tip = document.createElement("div");
  tip.setAttribute("style", "position:fixed;z-index:2147483647;left:50%;top:12px;transform:translateX(-50%);pointer-events:none;font:600 13px Segoe UI,system-ui;color:#fff;background:#111;border-radius:8px;padding:7px 12px;box-shadow:0 6px 24px rgba(0,0,0,.4)");
  tip.textContent = "Click the thing to hide on this site · Esc cancels · mouse wheel picks a bigger or smaller part";
  document.documentElement.append(box, tip);
  let target = null;
  let depth = 0;
  const esc = (s) => (window.CSS && CSS.escape ? CSS.escape(s) : s);
  const unique = (sel) => {
    try {
      return document.querySelectorAll(sel).length === 1;
    } catch (e) {
      return false;
    }
  };
  const stableId = (id) => id && !/\d{4,}/.test(id);
  const selectorFor = (el) => {
    if (stableId(el.id) && unique("#" + esc(el.id))) return "#" + esc(el.id);
    const parts = [];
    for (let e = el; e && e.nodeType === 1 && e !== document.documentElement && parts.length < 6; e = e.parentElement) {
      if (e !== el && stableId(e.id)) {
        parts.unshift("#" + esc(e.id));
        const sel = parts.join(" > ");
        if (unique(sel)) return sel;
        break;
      }
      let part = e.tagName.toLowerCase();
      const cls = [...e.classList].filter((c) => !/\d{3,}|^(is|has)-|active|hover|open|show|visible/.test(c)).slice(0, 2);
      if (cls.length) part += "." + cls.map(esc).join(".");
      const p = e.parentElement;
      if (p) {
        const same = [...p.children].filter((c) => c.tagName === e.tagName);
        if (same.length > 1) part += ":nth-of-type(" + (same.indexOf(e) + 1) + ")";
      }
      parts.unshift(part);
      const sel = parts.join(" > ");
      if (unique(sel)) return sel;
    }
    return parts.join(" > ");
  };
  const pickAt = (x, y) => {
    let el = document.elementFromPoint(x, y);
    if (!el || el === box || el === tip) return;
    for (let i = 0; i < depth && el.parentElement && el.parentElement !== document.body; i++) el = el.parentElement;
    target = el;
    const r = el.getBoundingClientRect();
    Object.assign(box.style, { left: r.left + "px", top: r.top + "px", width: r.width + "px", height: r.height + "px" });
  };
  let last = [0, 0];
  const move = (e) => {
    last = [e.clientX, e.clientY];
    pickAt(e.clientX, e.clientY);
  };
  const wheel = (e) => {
    e.preventDefault();
    depth = Math.max(0, Math.min(12, depth + (e.deltaY < 0 ? 1 : -1)));
    pickAt(last[0], last[1]);
  };
  let timer = 0;
  const done = (value) => {
    removeEventListener("mousemove", move, true);
    removeEventListener("click", click, true);
    removeEventListener("keydown", key, true);
    removeEventListener("wheel", wheel, { capture: true });
    box.remove();
    tip.remove();
    window.__pbZapping = false;
    clearTimeout(timer);
    resolve(value);
  };
  const click = (e) => {
    e.preventDefault();
    e.stopImmediatePropagation();
    if (target) done(selectorFor(target));
  };
  const key = (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      done(null);
    }
  };
  addEventListener("mousemove", move, true);
  addEventListener("click", click, true);
  addEventListener("keydown", key, true);
  addEventListener("wheel", wheel, { capture: true, passive: false });
  timer = setTimeout(() => done(null), 120000);
})
