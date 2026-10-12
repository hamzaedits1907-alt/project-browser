# AI features

NVIDIA's hosted API is the only provider. Everything below goes through the same path: the shell builds a request, the
Rust core checks it, attaches the API key (which only the core can read) and talks to `integrate.api.nvidia.com`.

| Feature | Where | What it does | What it sends |
| --- | --- | --- | --- |
| Chat | Assistant panel → Chat | Streams answers from the model you chose | Your messages; the page's text only with "Include this page" |
| Shortcuts | Assistant panel → chips | Explain selection, Translate to…, Extract data, Cited summary | The same as chat, with the page attached |
| Research | `browser://research` | Compares the pages you tick, or weighs them against a claim | Only the ticked pages' text |
| Project Memory | `browser://memory` | Notes, summaries and research you chose to keep | Nothing (it is local) |
| Actions | Assistant panel → Act | You describe a task, you review a plan, it runs the steps you leave ticked | The request to make the plan; page text only for steps that read the page |
| Restyle page | Act (or any plan with a restyle step) | Local, reversible CSS for the current view | Nothing to the site; the request to NVIDIA |

## Setting up (the key is always enterable)

The settings page names the stage you are at and the key field is usable at every stage:

`loading → noKey → keyEntered → keySaved → catalogLoading → (invalidKey | providerFailure | noUsableModels) → chooseModel → verified → ready`
(`modelUnavailable` can appear after the list loads if the saved model was retired or is not a chat model.)

* The model list (`GET /v1/models`) is **not** proof of a valid key: NVIDIA answers it the same for any key or none. The key
  is checked with a real one-token chat request when you press Connect or Test connection.
* A key NVIDIA rejects is never saved. A key that could not be checked because of the network, a timeout, a rate limit or a
  server error can be saved with **Save without checking**; the stage line then says it has not been verified.
* A missing key is never reported as a model problem, and a retired model is never reported as a key problem
  (`setupStage` in `aiModels.ts`, with tests).
* The key lives only in Windows Credential Manager (see [SECURITY.md](SECURITY.md)).

## Cost and context controls

* **Most page text per request** (Settings → AI assistant → Context and cost, default 24,000 characters ≈ 6,000 tokens).
  The core enforces it on every request that carries page text: `ai::check_context` refuses a request larger than the limit
  plus a fixed 16,000-character allowance for instructions and conversation, whatever the shell asked for.
* Page text is extracted without menus, footers, sidebars, scripts, hidden elements and form fields
  (`native/page_extract.js`), so the budget goes to content.
* Research shares the limit evenly between the ticked pages and shows an estimate (about four characters per token; it is
  an estimate, not NVIDIA's count) before you press Run.
* Replies are limited to 4,096 tokens, stop after three minutes or when nothing arrives for 90 seconds, and are cut at
  60,000 characters. **Stop/Cancel** cancels a request at once, even before NVIDIA has answered (polled every 0.4 s).
* The selected model is reused for everything. Model-specific refusals (retired, not a chat model) are reported with the
  model's own explanation.
* Nothing is sent in the background. No page, tab list or browsing history is sent unless you select it.

## Research Constellation and Claim Battle

You tick pages (private tabs, browser pages and asleep/unloaded tabs can't be ticked; the core refuses them again when it
reads). The model is told to answer in JSON in which every statement names a source id (S1, S2…) and **quotes the exact
words** it relied on. The shell then does what the model cannot be trusted to do:

* A statement that names no page you selected is **dropped** (and counted: "N statements were removed").
* Each quote is searched in the exact text that was sent (ignoring case, spacing and typographic quote/dash styles; "…"
  quotes are checked piece by piece in order; quotes shorter than 12 characters are never accepted). Found → ✓ *Page
  says*. Not found → the statement stays but is marked *Unverified* and shows that the quote wasn't found.
* A statement the model marks `inferred` is labelled *Model's inference*, never as something a page says.
* **Claim Battle** only counts verified quotes as evidence. Unverified support or opposition is moved to *Uncertain*. The
  result is a *leaning* (supports / contradicts / mixed / insufficient), never "proven": pages repeating each other count
  once per site, "only one site backs this up" is stated, and a caveat says agreement between pages is not proof.
  Reliability notes are labelled as the model's judgement.
* If the reply isn't valid JSON you get an error and, on request, the raw text labelled unchecked, never as findings.

Limits: the check proves a quote is on the page, not that the page is right, and not that the model drew the right
conclusion from it. It does not follow links. A page longer than its share of the context limit is read only up to that
point, and the result says so.

## Project Memory

Records are created only by a button you press: **New note**, **Save this answer to Project Memory** on an assistant
message, **Save to memory** on a research result, or a `save_to_memory` step you confirmed. A record is a title, text,
tags, the addresses and titles of source pages (never the pages' text), and links to other records. Stored in the profile
database (`memory_items`, schema v3) as plain text: it is your own notes, not credentials. View, search (every word must
match; case-insensitive in every language), edit, delete one, or delete all. It is not synced and is not sent anywhere unless
you copy it into a request. Clearing browsing data does not delete it (it is not browsing data); *Delete all* does.

## Actions (controlled, confirmable, cancellable)

1. You describe the task. The planner is told where you are (web page / browser page / private tab / none) and **nothing
   from any page**.
2. The model returns a JSON plan. `parsePlan` accepts only the tools below, with arguments that pass that tool's own
   check; a plan containing anything else is **rejected whole**.
3. You see the plan: every step in words, what it does ("Opens web pages", "Sends this page's text to NVIDIA", "Changes how
   this page looks (local)", "Saves to Project Memory (local)"), the exact addresses to be opened, and a checkbox per
   step. Steps that can't run here (page work in a private tab) start unticked with the reason.
4. **Run** executes only the ticked steps, in order, reporting each one. **Stop** ends the plan and cancels a request in
   flight. A failed step does not stop the others.
5. What ran is kept in the action history (what you asked and each step's outcome, never page text or results).
   *Clear history* removes it. History lives in this browser's local storage, not in the database.

| Tool | Effect | Notes |
| --- | --- | --- |
| `open_pages` | Opens up to 8 pages in background tabs | http/https only; no user name/password in the address; not localhost or private-network addresses. In a private tab the pages open as **private tabs** |
| `search_bookmarks` | Reads your bookmarks | local |
| `search_memory` | Reads Project Memory | local |
| `summarize_page`, `extract_from_page` | Reads the current page and asks the model | refused for private tabs, browser pages, files, asleep tabs |
| `compare_tabs` | Opens Research | reads nothing; you choose the pages there |
| `save_to_memory` | Saves a record | text from the plan, or the previous step's result |
| `restyle_page`, `undo_restyle` | Adds/removes one `<style>` element in the current view | CSS is checked twice (shell and core): no `url()`, `@import`, `image-set`, escapes, markup… |

There is no tool for the shell, PowerShell, the file system, form submission, downloads, passwords, cookies or account
changes. **Page content is untrusted:** pages are fenced and labelled as data in every prompt (the fence markers are
neutralised inside page text, titles and addresses), and replies and page text are shown and saved as text only. They are
never parsed as a plan, so nothing a page says can add a step, widen the list of tools or grant a permission.

## Page transforms

`restyle_page` injects one `<style id="__pb_ai_style">` into the active tab's top frame through the same script runner the
autofill uses. It affects only this view, only until undone or until the page reloads, and nothing is sent to the site.
The core checks the CSS again before applying it (`ai::check_page_css`) and embeds it as a JSON string literal.

## What is automatically tested

Frontend (vitest): setup stages; key-save rules; model catalog parsing and non-chat models; chat, planning and research
error/cancel states; quote verification; citation provenance (unknown sources, fabricated quotes, inference, single-site
support); prompt fencing against injection; the tool allowlist (shell/file/form tools, bad addresses, bad CSS, too many
steps); plan confirmation, unticked steps, private-tab behaviour, cancellation, history bounds; memory drafts; no secret in
any store. Rust (CI): context limit, reply bounds, readable-tab rules, CSS checks, the page-style script, memory CRUD,
search, links, persistence, and the NVIDIA parsers against a recorded live model list.

All of these use fake keys and mocked replies. **None of them is a request to NVIDIA.** A real key was not available when
this was built, so no feature here has been seen working against NVIDIA: see the manual list in
[VERIFY-WINDOWS.md](VERIFY-WINDOWS.md).
