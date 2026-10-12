// Fills a saved address profile into the page the user is looking at, using the standard HTML
// `autocomplete` attributes forms declare ("given-name", "postal-code", …). Run only after an explicit
// click in the shell, in the tab's top frame. It is a function expression: vault.rs wraps it as
// `(<this file>)(<arguments as JSON>)`.
//
// It re-checks the page's origin, never submits anything, and leaves fields that already hold text alone.
// It reports "filled:<n>", "no-form" or "origin-mismatch".
function (args) {
  "use strict";
  if (location.origin !== args.origin) return "origin-mismatch";

  var TOKENS = ["name", "given-name", "additional-name", "family-name", "email", "tel", "organization",
    "street-address", "address-line1", "address-line2", "address-level2", "address-level1", "postal-code", "country"];

  var fields = {};
  Object.keys(args.fields || {}).forEach(function (key) {
    if (TOKENS.indexOf(key) >= 0 && typeof args.fields[key] === "string") fields[key] = args.fields[key];
  });
  if (fields["street-address"]) {
    var lines = fields["street-address"].split(/\r?\n/);
    if (!fields["address-line1"]) fields["address-line1"] = lines[0];
    if (!fields["address-line2"] && lines.length > 1) fields["address-line2"] = lines.slice(1).join(", ");
  }

  var isVisible = function (el) {
    if (!el || el.disabled || el.readOnly || el.type === "hidden") return false;
    var rect = el.getBoundingClientRect();
    if (rect.width < 2 || rect.height < 2) return false;
    var style = window.getComputedStyle(el);
    return style.visibility !== "hidden" && style.display !== "none";
  };

  var notify = function (el) {
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  };

  var setText = function (el, value) {
    var proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value").set.call(el, value);
    notify(el);
  };

  var setSelect = function (el, value) {
    var wanted = value.toLowerCase();
    var options = Array.prototype.slice.call(el.options);
    var match = options.filter(function (o) { return o.value.toLowerCase() === wanted; })[0] ||
      options.filter(function (o) { return o.text.trim().toLowerCase() === wanted; })[0];
    if (!match) return false;
    el.value = match.value;
    notify(el);
    return true;
  };

  var count = 0;
  Object.keys(fields).forEach(function (token) {
    var value = fields[token];
    if (!value) return;
    var selector = ["input", "textarea", "select"].map(function (tag) { return tag + '[autocomplete~="' + token + '"]'; }).join(",");
    Array.prototype.slice.call(document.querySelectorAll(selector)).filter(isVisible).forEach(function (el) {
      if (el instanceof HTMLSelectElement) {
        if (setSelect(el, value)) count++;
      } else if (!el.value) {
        setText(el, value);
        count++;
      }
    });
  });
  return count > 0 ? "filled:" + count : "no-form";
}
