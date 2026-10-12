// Fills a saved login into the page the user is looking at. Run only after an explicit click on
// "Fill" in the shell, in the tab's top frame. It is a function expression: vault.rs wraps it as
// `(<this file>)(<arguments as JSON>)`.
//
// Rules this script keeps (see docs/CHROME-MIGRATION.md):
//   * it re-checks the page's own origin and does nothing on a mismatch (the page may have navigated
//     between the click and now);
//   * it never submits a form and never clicks anything;
//   * it only touches visible, enabled, writable fields;
//   * it reports what it did as a string: "filled:<n>", "username-only:1", "no-form" or "origin-mismatch".
function (args) {
  "use strict";
  if (location.origin !== args.origin) return "origin-mismatch";

  var isVisible = function (el) {
    if (!el || el.disabled || el.readOnly || el.type === "hidden") return false;
    var rect = el.getBoundingClientRect();
    if (rect.width < 2 || rect.height < 2) return false;
    var style = window.getComputedStyle(el);
    return style.visibility !== "hidden" && style.display !== "none";
  };

  // Sets the value the way a user's typing would, so frameworks that track input state notice it.
  var setValue = function (el, value) {
    var proto = HTMLInputElement.prototype;
    var setter = Object.getOwnPropertyDescriptor(proto, "value").set;
    el.focus();
    setter.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  };

  var inputs = Array.prototype.slice.call(document.querySelectorAll("input"));
  var isText = function (el) {
    var t = (el.getAttribute("type") || "text").toLowerCase();
    return (t === "text" || t === "email" || t === "tel") && isVisible(el);
  };
  var hintOf = function (el) {
    return [el.getAttribute("autocomplete"), el.name, el.id, el.getAttribute("aria-label"), el.placeholder].join(" ").toLowerCase();
  };
  var userScore = function (el) {
    var hint = hintOf(el);
    if (/\b(username|email)\b/.test(hint)) return 3;
    if (/user|e-mail|email|login|account|identifier/.test(hint)) return 2;
    return 0;
  };

  var password = inputs.filter(function (el) { return el.type === "password" && isVisible(el); })[0] || null;
  var filled = 0;

  if (password) {
    // The user-name field is the closest suitable field before the password field, in the same form if there is one.
    var scope = password.form ? Array.prototype.slice.call(password.form.querySelectorAll("input")) : inputs;
    var before = scope.filter(function (el) {
      return isText(el) && (el.compareDocumentPosition(password) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
    });
    var userField = before.length ? before[before.length - 1] : null;
    if (userField && args.username) {
      setValue(userField, args.username);
      filled++;
    }
    setValue(password, args.password);
    filled++;
    return "filled:" + filled;
  }

  // First step of a two-step sign-in: only the user name (or e-mail) field is on the page.
  if (args.username) {
    var best = inputs.filter(isText).sort(function (a, b) { return userScore(b) - userScore(a); })[0];
    if (best && userScore(best) > 0) {
      setValue(best, args.username);
      return "username-only:1";
    }
  }
  return "no-form";
}
