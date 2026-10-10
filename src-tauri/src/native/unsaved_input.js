(() => {
  // Does this page hold data the user typed and has not submitted? Asked before a tab's webview is destroyed.
  // When in doubt it answers yes: any error counts as "has input" (the caller then sleeps the tab instead of unloading it).
  try {
    for (const el of document.querySelectorAll('input, textarea, select, [contenteditable]')) {
      if (el.matches('[contenteditable="false"]')) continue;
      const tag = el.tagName;
      if (tag === 'INPUT') {
        const type = (el.type || '').toLowerCase();
        if (['hidden', 'button', 'submit', 'reset', 'image', 'file'].includes(type)) continue;
        if (type === 'checkbox' || type === 'radio') {
          if (el.checked !== el.defaultChecked) return true;
          continue;
        }
        if (el.value !== el.defaultValue) return true;
      } else if (tag === 'TEXTAREA') {
        if (el.value !== el.defaultValue) return true;
      } else if (tag === 'SELECT') {
        const options = Array.from(el.options);
        if (el.multiple) {
          if (options.some((o) => o.selected !== o.defaultSelected)) return true;
        } else {
          // A single select without a `selected` attribute shows its first option: that is its default, not a change.
          let initial = options.findIndex((o) => o.defaultSelected);
          if (initial < 0) initial = options.length ? 0 : -1;
          if (el.selectedIndex !== initial) return true;
        }
      } else if (el.isContentEditable && (el.textContent || '').trim().length > 0) {
        return true;
      }
    }
    return false;
  } catch (e) {
    return true;
  }
})()
