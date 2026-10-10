# Memory and CPU benchmark

A repeatable way to measure how much memory and CPU a browser uses, **from outside the browser**, so that Project
Browser, Chrome, Edge, Brave or Opera GX can be measured by exactly the same procedure and compared fairly.
`scripts/benchmark.ps1` does it; this page says what it measures and how to read it.

## What is measured

For each tab count (1, 5 and 10 by default) the script:

1. starts the browser with a **brand-new, empty profile** (no extensions, no sign-in) and the first *N* pages of a fixed list,
2. waits 30 s for the pages to load and 30 s to settle, then samples for an idle window (default 60 s) every 5 s,
3. in each sample adds up, over the browser's **whole process tree** (the main process and every child, each counted
   once by PID):
   * **memory**: private working set, the number Task Manager shows as "Memory": what the processes hold themselves, with
     shared pages (for example shared DLLs) not counted again,
   * **CPU**: processor time as a percentage of the whole machine (all logical CPUs),
4. closes the browser and writes a CSV of every sample plus a JSON summary (median, max, mean, process count).

Not measured: GPU (no reliable per-process source on Windows without extra tooling), network, battery, page-load speed.

The in-app Performance page measures with the same kind of counters (private working set per process, CPU time deltas) but
it is a live view for users; the benchmark is separate on purpose, so nothing inside the browser polls while it measures.

## Pages

A fixed list of light, static pages (`example.com`, a Wikipedia article, w3.org, gnu.org, httpbin, mozilla.org, MDN,
Hacker News, iana.org, kernel.org); the first 1, 5 or 10 are opened. They are chosen so that the test measures the browser
rather than a site's ads or video. **Real browsing with heavy sites will use much more memory than these numbers.**

## Rules for a fair comparison

* Same PC, nothing else running (close other browsers, no builds, no downloads, no antivirus scans), same power plan.
* Run every scenario **at least 3 times** and compare the medians; the spread between runs is part of the result.
* Same page list and timings for every browser. Do not sign in or add extensions.
* Note the browser versions (the script records them) and the Windows build.
* A number is only a statement about *this* page list, *these* tab counts, *this* PC.

```text
scripts\benchmark.ps1 -Browser project-browser -Exe "$env:LOCALAPPDATA\Project Browser\project-browser.exe"
scripts\benchmark.ps1 -Browser project-browser -Exe "...\project-browser.exe" -Gaming   # Gaming Mode: background tabs sleep
scripts\benchmark.ps1 -Browser chrome -Exe "C:\Program Files\Google\Chrome\Application\chrome.exe"
scripts\benchmark.ps1 -Browser edge   -Exe "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
```

Project Browser is started with `--load-all` (every startup URL loads, hidden, instead of only the last) and, for the
Gaming scenario, `--gaming` (Gaming Mode on for that session). Both are start-up switches; neither is saved in a profile.

## Results

Measured on 2026-10-10 on one PC: Windows 11 Pro (build 26200), 12 logical CPUs, 15.9 GB RAM, WebView2 runtime
154.0.4258.62, Project Browser CI builds of 0.1.1 and 0.1.2, nothing else running. Three repetitions per row; each cell
is the median of that repetition's 12 samples (60 s window, 5 s apart). "Range" is the lowest and highest repetition.
Page list as in the script (the first 1, 5 or 10). **Nothing else was measured: there are no Chrome, Edge, Brave or Opera GX numbers here,
and none of this says Project Browser uses less memory than any other browser.**

Total memory of the whole process tree, MB (private working set):

| Tabs | Normal | Gaming Mode, background tabs **sleep** | Gaming Mode, background tabs **unload** |
| --- | --- | --- | --- |
| 1 | 117 (117-120) | 135 (114-163, noisy: single tab, nothing to sleep) | n/a |
| 5 | 302 (301-306) | 301 (300-304) | 290 (286-291) |
| 10 | 385 (351-390) | 348 (344-377) | 313 (310-324) |

Processes (median): normal 8 / 12 / 17 for 1 / 5 / 10 tabs; unload 9 for both 5 and 10 tabs. Idle CPU was 0-0.8 % of the machine in every run.

What this does and does not show:

* **Idle cost is low and flat:** with every page loaded and untouched, CPU was effectively zero.
* **Sleeping saves very little here.** Suspending a tab stops its CPU use and trims memory, but these pages are tiny and static; at 5 tabs the
  difference (302 vs 301 MB) is smaller than the spread between repetitions, and at 10 tabs the ranges (normal 351-390, sleeping 344-377)
  overlap. Sleeping tabs matter for CPU and for heavy pages; that is **not** measured here.
* **Unloading saves real memory:** at 10 tabs about 72 MB less than normal (313 vs 385 MB, -19 %, ranges do not overlap) and half the processes.
  The price: an unloaded page reloads when opened (see the sleeping/unloading section of ARCHITECTURE.md).
* The browser alone (one tab) is about 117 MB across 8 processes; most of the floor is WebView2's shared processes, which do not shrink to zero.
* An earlier set of unload runs (0.1.1) was discarded: a bug treated an untouched drop-down list as typed-in text and kept retrying one tab, so the
  runs were inconsistent. It is fixed in 0.1.2 (tests: `src/lib/unsavedInput.test.ts`); the table above is from 0.1.2.
* A first set of runs was also discarded because other work was running on the PC during it (CPU column showed it). That is why the rules above say
  "nothing else running".

Raw per-sample CSV and JSON summaries are written by the script (`bench-results/`); they are not kept in the repository.
