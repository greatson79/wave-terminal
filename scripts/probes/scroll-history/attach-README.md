# Fix C attach probe — preparation only

SKILLS_USED: ponytail (lite), test-driven-development, hallucination-guard, code-review-and-quality.

Theo GO kai#55ff41 via Ben kai#a1e900 authorizes the daemon-only bounded-ring fix and preparation. This probe is ported from c7956e0; exact fixture bytes, xterm/headless5.5.0, dependency lock, marker comparator and child remain the same. Local branch `fix/0.3.8-scroll-history` starts at9b90f24 (contains c80e2df). No push/dispatch or local daemon execution is performed here.

## One job, three measurements

Each variant creates a new seat on an isolated hosted-CI daemon, with a primary xterm client and a continuously fast reference client attached BEFORE playback. Both receive BEGIN before the child stress gate opens. Stress stays70×35. The child flushes each512-byte payload write without artificial delay, emits END, then waits for the tail gate. This establishes an exact shared post-ConPTY live window that excludes every attach snapshot.

- FAST keeps the prior256-copy control and reads continuously. Its marker-delimited output must equal the reference, with zero gap counters.
- SLOW writes48fixture copies, pauses its actual socket for5seconds, then drains. Its measured reference window must remain below4MiB; notification-lag count must be positive; primary/reference slices must be byte-identical with zero missing bytes and no gap events. If the environment never exercises notification lag, the run is INDETERMINATE rather than a claimed recovery pass. Rust tests on the actual shared pump provide deterministic >256-notification pressure independently of socket chunk coalescing.
- CAP writes256copies while the primary pauses5seconds. Its measured reference window must exceed4MiB; the primary must receive the daemon's `[Wave output gap: ...; re-attaching]` marker followed by EOF, and gap-events/gap-bytes counters must be positive. After queued xterm writes complete and BEFORE the automatic reconnect snapshot, the harness requires `[Wave output gap:` in the rendered active viewport rows (a warning hidden in the normal buffer does not count) and records `gapVisible`. The harness automatically reattaches using the existing UI policy: same daemon PID, live seat, at most5attempts spaced500ms; daemon absence consumes wait attempts without attaching, a changed PID stops. The ACK is followed by another PID check. This is an explicitly tested headless wire model of `src-tauri/src/main.rs::stream_end_action`, not execution of Tauri/WebView2.

After the pre-reattach comparison (or CAP fallback+automatic reattach), the child emits one original fixture at512bytes/40ms. The primary fits/resizes60×35 after one-third of the tail and makes one deliberate500ms reattach after55%, retaining the same xterm. CAP therefore has an extra automatic recovery before this deliberate reconnect. Child metadata proves exact emitted payload bytes/chunks, cumulative SHA256, original fixture SHA256, marker byte count, PS5.1 and final60×35 console geometry.

Original fixture:28352bytes, SHA256 `a6770aba1c94852f31f354c183a183a3c681fcd60f312ad26cc15f771949794d`. SLOW stress input=1360896bytes; FAST/CAP stress input=7258112bytes. Ring limit is exactly4194304retained raw output bytes; ConPTY output sizes differ, so CI checks the measured reference window instead of assuming input/output equality.

## Counters and evidence

Existing `surface.list` exposes per-seat `attach_lagged_chunks`, `attach_gap_events`, `attach_gap_bytes`; the probe saves that response. These aggregate both attach receivers. Notification misses may be recovered from the byte ring; they do not mean unrecoverable bytes. Gap counts/bytes distinguish fallback beyond the byte bound.

Each variant saves `received.bin`, continuous `reference.bin`, `result.json` (marker comparison, counter response, automatic-policy events, pause/reference socket counts, resize chronology, label counts), `child-complete.json`, `raw-baseline.json`, `waved.log`, and `lifecycle.json`. Failure also saves raw/event evidence. Output folders must be new; random temp roots remain. Workflow runs all3variants and fails after the loop if any is incomplete; artifact collection runs regardless.

The raw fixture judge must retain100/100 before the probe. Actual BEFORE/AFTER Mac replay evidence is produced by the core agent's daemonless test of the same shared Rust pump, captured through `SCROLL_CAPTURE_PATH` and judged with the pinned headless renderer. Merely judging the unchanged input twice is not accepted as that evidence. No daemon binary is launched locally.

## Checks and future CI

Local checks cover split ACK/raw bytes, strict wrapped-label counting, invalid ACK, exact child gates/repeated bytes/hash/counts, local runtime rejection, marker equality/omission/missing delimiters, and exact retry/wait policy including its maximum budget. Workflow also runs `cargo test --locked --bin waved output::tests -- --nocapture` before building the real daemon.

The workflow only registers on push to `fix/0.3.8-scroll-history`; its Windows job runs only for an independently authorized `workflow_dispatch`. Approval must bind the final exact SHA, branch, numeric workflow ID and one dispatch. Runtime rejects execution outside a disposable GitHub-hosted Windows runner before creating evidence or starting a process. Windows known-folder home ignores HOME/USERPROFILE; those env overrides are not claimed to isolate a real user's account/adapters. This harness is never an owner-PC command.

## Limits

Whole-run child-minus-client byte delta is diagnostic only: ConPTY transforms bytes, markers/snapshots add bytes, deliberate reconnect omits live bytes. Only the common marker window supports exact relative-delivery comparison. A strict shorter subsequence is an omission relative to the reference; both receivers could still miss the same bytes. The SLOW positive notification counter and exact shared-pump Rust tests are needed to establish exercised recovery. Final repeated100-label sets prove neither earliest-copy retention nor tail survival because labels repeat and scrollback is capped at5000lines.

This probe bypasses Tauri event serialization, physical FitAddon measurements, WebView2 and the actual3-pane GUI. It models70-column geometry, replays synthetic Mac-derived bytes rather than live Windows Claude, and tests automatic fallback via the headless policy model. Ring-overflow claims require measured counter/marker/EOF evidence; sending256copies or observing many socket chunks alone does not prove overflow. Local Mac/Windows GUI and learner-PC wheel/previous-message acceptance remain separate required evidence.
