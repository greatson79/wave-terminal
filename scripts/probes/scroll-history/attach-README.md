# Real waved attach/resize experiment — preparation only

SKILLS_USED: ponytail (lite), hallucination-guard, debugging-and-error-recovery, test-driven-development.

Smallest complete path: on a disposable GitHub-hosted Windows runner only, build the existing waved binary, use its real `surface.create` / `surface.attach` / `surface.resize` RPCs, and render received raw bytes in the already installed xterm/headless 5.5.0. No application, vendored PTY, existing judge, dependency, or existing workflow changes.

Source anchors: UI writes attach bytes directly (`ui/src/main.ts:2128–2130`) and fits xterm before invoking daemon resize (`ui/src/main.ts:2154–2158`); backend ACK/raw stream (`src-tauri/src/main.rs:1652–1699`), reconnect delay500ms (`src-tauri/src/main.rs:1598`); real daemon snapshot and live broadcast (`src/bin/cysd/main.rs:1944–1969`).

The isolated child emits the exact committed fixture, SHA256 `a6770aba1c94852f31f354c183a183a3c681fcd60f312ad26cc15f771949794d`, 512 bytes per40ms. Client starts120×35, attaches before playback, fits70×35, starts playback, resizes60×35 after one-third of emitted bytes, then drops/re-attaches once after55% of emitted bytes with a500ms gap while emission continues. The SAME xterm survives. One gap intentionally models what the app cannot replay. This is one combined experiment, not independent attribution of resize versus reconnect.

A child progress sidecar drives the mid-stream milestones rather than wall-clock guesses. Exact child bytes/hash/completion and (Windows) PS5.1/final60×35 geometry are mandatory; missing evidence makes the command fail rather than count as retention loss. Raw fixture must retain100 labels before the experiment. Actual retention is a measurement; fewer than100 labels does not itself mean harness failure.

Retained evidence in a NEW output folder: `received.bin`, `result.json` (write chunks with attachment ID/time/base64, resize/checkpoint chronology and wrapped-line-aware normal/active label counts), `child-complete.json`, `raw-baseline.json`, `waved.log`, and `lifecycle.json`. The random isolated temp root is preserved. Existing output directories are refused.

Checks performed locally: client self-test first failed because implementation did not exist (ATTACH_PREP_RED.log), then passed split ACK/raw preservation, invalid ACK, same-terminal snapshot retention, narrow resize, wrapped-label counting, known ESC[3J loss, and exact child start-gate/output/completion. Both Node files pass `node --check`; `git diff --check` passes. No waved/cysd process was started locally. No PS5.1/ConPTY/WebView2/real daemon run has been claimed.

Future disposable hosted-CI runner command after separate approval (never an owner-PC command):

```powershell
cargo build --locked --bin waved
node scripts/probes/scroll-history/attach-selftest.cjs
node scripts/probes/scroll-history/attach-repro.cjs (Get-Item target/debug/waved.exe).FullName evidence-attach
```

The new `.github/workflows/scroll-attach-repro.yml` registers on a push restricted to `repro/0.3.7-scroll-attach`; its only job runs on `workflow_dispatch`. As with the preceding experiment, registration/skipped is not a test pass. Approval must bind the final commit, exact branch, workflow ID, and ONE dispatch. Nothing has been pushed/dispatched.

Limits: uses the real daemon seat/attach path but bypasses Tauri event serialization, browser DOM/FitAddon measurement, and WebView2; headless `resize()` follows the app's order. It models one explicit transport gap, not a forced256-chunk broadcast overflow. It replays synthetic Mac-derived bytes, not live Windows Claude output. The requested3-pane layout is represented by70-column geometry, not three independent GUI panes. Runtime rejects local Windows, Mac, and self-hosted execution before creating evidence or spawning processes. Windows known-folder home resolution ignores HOME/USERPROFILE; those overrides do not isolate a real user's `.cys/accounts.json` or custom adapters. Disposable hosted CI, a unique pipe, and fresh pack/account directories form this experiment's boundary. Local Mac daemon baseline remains unrun under the no-direct-daemon rule.
