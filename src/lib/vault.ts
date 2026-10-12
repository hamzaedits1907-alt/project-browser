// Small pure helpers for the saved-logins UI: searching the list, deciding where filling is offered, and
// the words used after a fill attempt.

import type { FillResult, LoginSummary, Tab } from "./types";

/** Case-insensitive match on the site and user name; every word must match. */
export function filterLogins(logins: readonly LoginSummary[], query: string): LoginSummary[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...logins];
  return logins.filter((l) => {
    const hay = `${l.origin} ${l.username}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** What to tell the user after a fill attempt. Every outcome is reported, including "nothing was filled". */
export function fillMessage(r: FillResult): string {
  switch (r.state) {
    case "filled":
      return `Filled ${r.fields} field${r.fields === 1 ? "" : "s"}. Check the form, then submit it yourself.`;
    case "usernameOnly":
      return "Filled the user name. Continue to the next step, then fill again for the password.";
    case "originMismatch":
      return "The page changed after you opened the menu, so nothing was filled.";
    default:
      return "No form to fill was found on this page. Forms inside frames can't be filled.";
  }
}

const LOCAL_HTTP = /^http:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/;

/** Whether a tab's page is one where saved details may be offered at all (https, or this computer). */
export function canOfferFill(tab: Tab | null): tab is Tab {
  if (!tab || tab.error) return false;
  return tab.url.startsWith("https://") || LOCAL_HTTP.test(tab.url);
}

/** A saved login's date for display; "never" when it was not used yet. */
export function usedText(ms: number | null): string {
  return ms ? new Date(ms).toLocaleDateString() : "never";
}
