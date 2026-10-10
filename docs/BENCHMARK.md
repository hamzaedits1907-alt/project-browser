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

*(see below; nothing here is a claim about other browsers)*
