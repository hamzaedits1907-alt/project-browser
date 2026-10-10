# The 0.1.0 startup crash: what was found and how

**Symptom.** Version 0.1.0 started, drew nothing useful, and died about 3 seconds later, every time. Windows
logged `project-browser.exe 0.1.0.0`, faulting module `EmbeddedBrowserWebView.dll` 154.0.4258.62, exception
`0xC0000005` (access violation), fault offset `0xBB9C0`. The app log stopped after `session restored with 1 tabs`.

## How it was narrowed down (evidence, not guesses)

| Step | Result |
| --- | --- |
| Reproduced with the installed exe on the affected PC, no changes | Crash after 3.15 s, same module and offset |
| Fresh WebView2 user-data folder (`WEBVIEW2_USER_DATA_FOLDER`, confirmed in the WebView2 process command line) | Still crashes: **not a corrupted profile** |
| WebView2 SmartScreen feature switched off (`--disable-features=…msSmartScreenProtection`, confirmed in the command line) | Still crashes: **not SmartScreen** |
| Ran the exe under the Windows debug API (a small read-only harness, no code injected) | Fatal fault is on the **UI thread**, a **read of address `0xFFFFFFFFFFFFFFFF`**, 0.6 s after launch; the call chain is `WebView2 DLL → our executable (an event callback) → our executable calling back into WebView2 → fault` |
| Disassembled the two return addresses in our executable | A closure that makes two consecutive COM vtable calls on one object: slot 3 returns a BOOL, slot 4 returns a pointer |
| Rebuilt the same commit through the same `tauri build` path with symbols kept, matched the function by size and shape, resolved it through the PDB | The function is `project_browser_lib::native::collect_processes::closure_env$5`, our completion callback for `GetProcessExtendedInfos`; the slots are `ICoreWebView2FrameInfoCollectionIterator::HasCurrent` / `GetCurrent` |
| Why at startup | The restored New Tab page subscribes to a one-off memory reading (`perf_subscribe`), which runs this sampler about 100 ms after the UI is ready |

## Root cause

`native::collect_processes` walked the frame list of each WebView2 process with

```rust
ext.AssociatedFrameInfos().and_then(|f| f.GetIterator())
```

The frame collection `f` was released as soon as `GetIterator` returned, while the iterator was still used. If the
iterator does not hold its own reference to the collection, `GetCurrent` then reads freed memory. That is consistent
with the observed read of `0xFFFF…FFFF` inside the runtime, and it is the fix that was applied.

## What is proven and what is not

* **Proven:** the crash is in that function, on that code path, and is reproducible. Rebuilt with the collection (and
  every other COM object in the chain) kept alive until the walk ends, the same executable on the same PC and the same
  runtime survives: **3 of 3 runs** with a fresh profile, each logging `frame info walk finished`, while the unfixed
  0.1.0 run through the same script crashed. (`scripts/smoke-startup.ps1`.)
* **Not proven:** whether the runtime itself also has a bug in that iterator that the lifetime fix merely avoids. Both
  explanations fit the evidence; only the first is confirmed to remove the crash. No Microsoft report was found.

## Defence in depth (so one bad runtime cannot take the browser down again)

A native access violation cannot be caught. Before the first frame walk of a run the app writes a marker file
(`frame-info-guard.txt` in the app data folder); after the walk it removes it. If the process dies mid-walk the marker
is still there at the next start, and frame infos then stay **off for that WebView2 version** (the Performance page says
per-tab memory is unavailable instead of crashing). A different runtime version tries again.
`PROJECT_BROWSER_FRAME_INFO=off|on` overrides this for diagnosis.

## Regression coverage

* `native::tests`: the guard's decision table and the marker file's life across simulated runs.
* `scripts/smoke-startup.ps1`, run by CI on the freshly built exe: it must stay alive, log the startup stages, finish the
  first process sample and leave no Windows crash event. Run against the 0.1.0 binary it fails with `0xC0000005`.
* Every startup stage is logged (`%LOCALAPPDATA%\com.projectbrowser.desktop\logs\project-browser.log`), so the last
  line says how far a failing start got.
* The lifetime itself cannot be unit-tested without a WebView2 runtime; the smoke test is the check.
