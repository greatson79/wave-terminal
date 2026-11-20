# Real waved attach/resize experiment — preparation only

SKILLS_USED: ponytail (lite), hallucination-guard, debugging-and-error-recovery, test-driven-development.

Smallest complete path: on a disposable GitHub-hosted Windows runner only, build the existing waved binary, use its real `surface.create` / `surface.attach` / `surface.resize` RPCs, and render received raw bytes in the already installed xterm/headless 5.5.0. No application, vendored PTY, existing judge, dependency, or existing workflow changes.

Source anchors: UI writes attach bytes directly (`ui/src/main.ts:2128–2130`) and fits xterm before invoking daemon resize (`ui/src/main.ts:2154–2158`); backend ACK/raw stream (`src-tauri/src/main.rs:1652–1699`), reconnect delay500ms (`src-tauri/src/main.rs:1598`); real daemon snapshot and live broadcast (`src/bin/cysd/main.rs:1944–1969`).

The same job runs FAST and SLOW separately. Each run creates one real seat and TWO raw attach clients: the primary feeds xterm; the reference always drains as fast as Node can. Both attach before playback and receive an exact unique printable BEGIN token before the stress gate opens. The child then emits256 copies of the committed fixture, SHA256 `a6770aba1c94852f31f354c183a183a3c681fcd60f312ad26cc15f771949794d`, flushing each512-byte write with no artificial delay. The client starts120×35 and fits70×35 before playback; both clients remain70cols throughout stress. In SLOW, the actual primary socket is paused for at least5seconds while this stress stream runs; the reference is never paused. Pause/resume events record child progress, primary counts and reference socket-data byte/chunk counts during the pause.

The child emits a unique END token and holds a tail gate. Before ANY reattach, the host waits until both clients have BEGIN/END and compares their byte slices between those tokens, independently of socket chunk boundaries. All initial snapshots precede BEGIN; the one reattach snapshot follows this comparison. Missing/reversed/duplicate markers mean an indeterminate comparison, never equality. A strict shorter byte subsequence is reported as an omission relative to the reference; arbitrary differences remain differences, not a blanket drop claim. The comparison uses post-ConPTY daemon output from two real attach consumers; it does not attribute a discrepancy to one source line without runtime instrumentation.

After comparison, the child emits one more original fixture at512bytes/40ms. The host fits/resizes60×35 after one-third of this paced tail, recording the child progress that proves the tail remains in flight. It then drops/re-attaches the primary once after55% of the tail with a500ms gap, preserving the SAME xterm; the reference remains continuous. Child payload hash covers all257 fixture copies actually written; the original fixture hash and marker byte count are recorded separately. This is one combined stress/resize/reattach experiment, not independent attribution of the final label count to each operation.

A child progress sidecar drives the stress/tail milestones. Exact child payload bytes/chunks/hash/completion and (Windows) PS5.1/final60×35 geometry are mandatory; missing evidence makes the command fail rather than count as retention loss. Raw fixture must retain100 labels before the experiment. `delivery` and `referenceDelivery` report childBytes/childChunks, clientBytes/clientChunks, and childMinusClientBytes. That delta is diagnostic only: ConPTY transforms bytes, snapshots add bytes, markers add bytes, and the deliberate reconnect omits live bytes. It is not drop proof. Child write chunks, Node socket-data chunks, and daemon broadcast chunks are different units.

Retained evidence for each FAST/SLOW run in a NEW output folder: `received.bin`, continuous `reference.bin`, `result.json` (marker comparison, primary write chunks with attachment ID/time/base64, reference socket event counts, pause/resize/checkpoint chronology and wrapped-line-aware normal/active label counts), `child-complete.json`, `raw-baseline.json`, `waved.log`, and `lifecycle.json`. Raw/event evidence persists on failure too. The random isolated temp root is preserved. Existing output directories are refused.

Checks performed locally: client self-test first failed because implementation did not exist (ATTACH_PREP_RED.log), then passed split ACK/raw preservation, invalid ACK, same-terminal snapshot retention, narrow resize, wrapped-label counting, known ESC[3J loss, and exact child start-gate/output/completion. All four Node files pass `node --check`; `git diff --check` passes. Revised checks cover paired marker windows, strict omissions, invalid/overlapping delimiters, exact gated repeated payload bytes/hash/chunks, diagnostic metrics, and local daemon refusal (ATTACH_COMPARE_GREEN_R2.log, ATTACH_STRESS_GREEN.log). No waved/cysd process was started locally. No PS5.1/ConPTY/WebView2/real daemon run has been claimed.

Future disposable hosted-CI runner command after separate approval (never an owner-PC command):

```powershell
cargo build --locked --bin waved
node scripts/probes/scroll-history/attach-selftest.cjs
node scripts/probes/scroll-history/attach-compare-selftest.cjs
New-Item -ItemType Directory -Path evidence-attach
$failed = $false
foreach ($variant in 'FAST','SLOW') {
  node scripts/probes/scroll-history/attach-repro.cjs (Get-Item target/debug/waved.exe).FullName "evidence-attach/$variant" $variant
  if ($LASTEXITCODE -ne 0) { $failed = $true }
}
if ($failed) { throw 'At least one variant incomplete' }
```

The new `.github/workflows/scroll-attach-repro.yml` registers on a push restricted to `repro/0.3.7-scroll-attach`; its only job runs on `workflow_dispatch`. As with the preceding experiment, registration/skipped is not a test pass. Approval must bind the final commit, exact branch, workflow ID, and ONE dispatch. Nothing has been pushed/dispatched.

Limits: uses the real daemon seat/attach path but bypasses Tauri event serialization, browser DOM/FitAddon measurement, and WebView2; headless `resize()` follows the app's order. Stress tries to pressure the256-chunk broadcast ring, but a5second pause and256fixture copies do not prove that ring overflowed: pipe/OS buffers and broadcast coalescing can absorb output. Reference socket-data counts are not daemon broadcast counts. `overflowProven:false` remains explicit; equal windows cannot exonerate the silent-Lagged path, and both consumers could miss the same chunks. Repeated labels are not unique across257copies and xterm retains only5000scrollback lines, so the final100-label set proves neither earliest-copy retention nor paced-tail survival. The matched pre-reattach stress window establishes only relative transport equality/omission.

It replays synthetic Mac-derived bytes, not live Windows Claude output. The requested3-pane layout is represented by70-column geometry, not three independent GUI panes. Runtime rejects local Windows, Mac, and self-hosted execution before creating evidence or spawning processes. Windows known-folder home resolution ignores HOME/USERPROFILE; those overrides do not isolate a real user's `.cys/accounts.json` or custom adapters. Disposable hosted CI, a unique pipe, and fresh pack/account directories form this experiment's boundary. Local Mac daemon baseline remains unrun under the no-direct-daemon rule.
