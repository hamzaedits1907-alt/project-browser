# Project Shield

Request-level ad and tracker blocking, built into the browser.

## What it does

* Every sub-resource request a page makes (scripts, images, fonts, XHR/fetch, media, WebSockets, workers) is offered to
  WebView2's `WebResourceRequested` event (`native::hook_shield`). A request that matches a block rule is answered with an
  empty `403` before it leaves the computer. Sub-frame navigations are cancelled through `FrameNavigationStarting`.
* **Top-level page loads are never blocked.** WebView2 reports a main-frame load and an iframe with the same resource
  context, so iframes are handled by `FrameNavigationStarting` instead.
* Rules: the network part of the Adblock Plus syntax: `||host^`, `|…|` anchors, `*`, `^`, `@@` exceptions, and the options
  `$third-party`, `$~third-party`, resource types (`script`, `image`, `stylesheet`, `xmlhttprequest`, `subdocument`,
  `media`, `font`, `ping`, `websocket`, `other`), `$domain=`, `$important`. Rules with options the engine does not implement
  (`popup`, `redirect=`, `csp=`, `removeparam=`, regular expressions …) are **counted and skipped, never approximated**.
  Element-hiding (cosmetic) rules are skipped as well: there is no cosmetic filtering in this version.
* Lists: EasyList and EasyPrivacy (https://easylist.to, dual-licensed CC BY-SA 3.0 and GPL v3, by the EasyList authors),
  plus 25 well-known ad networks built into the program, so blocking works before the first download. About 110,000
  network rules are in use when both lists are on.
* Matching is indexed: each rule is filed under its rarest whole word, so a request only looks at the few rules sharing a
  word with its address (tested with 40,000 rules). Nothing is parsed per request and nothing polls.
* Controls: global switch, per-site switch (shield button in the toolbar), list switches, your own rules (with `@@`
  exceptions), a "test a request" box, and a blocked-count badge per page.
* Counts: per tab, per site (this session) and a lifetime total. Only counts are stored, never addresses. Private tabs are
  not counted per site or in the lifetime total.
* Protection of the browser itself: requests to and pages on `localhost` (which is how the browser's own interface is
  served) are never filtered, and the shell webview is not hooked at all.

## Updating lists

Lists are fixed in code (`shield/lists.rs`), fetched over HTTPS only, with a conditional request (`ETag` /
`Last-Modified`). A download is replaced into use only after it is checked: valid text, a list header and at least 1,000
rules, at most 12 MB. The stored list is replaced atomically (temporary file, then rename). A failed or invalid download is
reported in the settings page and the previous copy stays in use. Lists are checked once, 20 seconds after start, when
they are missing or older than three days, and when the user presses "Update lists".

## What has and has not been checked

* **Tested in CI:** the parser and matcher (anchors, wildcards, separators, exceptions, `$important`, type, party and
  domain options, malformed lines, 40,000-rule index), list validation and atomic install, conditional downloads and every
  failure kind against a local server, configuration cleaning, per-site and global switches, counters.
* **Not checked:** blocking in a real WebView2 window, compatibility with real sites, memory and CPU of the real lists,
  and any comparison with uBlock Origin or Brave Shields. Do not read the rule count as protection quality. Steps to check
  it are in `docs/VERIFY-WINDOWS.md` (section 0.4.0, S1–S6).

## Known limits

No element hiding; no `popup`, `redirect` or `csp` rules; iframes are blocked at navigation time only; a request made
before the page's address is known is treated as first-party; service-worker requests are covered only when the installed
WebView2 runtime offers `AddWebResourceRequestedFilterWithRequestSourceKinds`.
