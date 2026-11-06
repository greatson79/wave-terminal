# Teams M1 Windows baseline assertions and test-home isolation

Scope: approved Pulse/Theo amendment dated 2026-11-06 21:51. The three Windows core-library failures are recorded separately; no assertion is changed, weakened, skipped or cfg-disabled by M1.

Observed run: `42219596887`, candidate combined M1 branch, Windows core library **145 passed / 3 failed**; macOS job passed. Local source log: `m1_ci/run_42219596887.log`, lines 3096–3118. This source comparison is evidence of pre-existing code, not a claim that an unchanged Windows baseline has already been executed.

Pinned baseline: v0.3.1 `85f160fde191593c290293e6295961f712ff3e47`.

| Failed test | Baseline source and cause | M1/environment assessment |
|---|---|---|
| `pack::tests::install_from_iter_equivalent_to_install` | `src/pack.rs:2124` puts native path strings into fingerprint keys, then `:2171–2177` probes slash-separated strings. Windows keys contain backslashes. The entire pack.rs blob is unchanged (`8d8b845c52b9076ae363aec8d041b93ab932c74e`). | Existing Windows assertion mismatch. The explicit per-thread pack/config fixture does not depend on HOME/USERPROFILE. M1 does not modify product pack logic. |
| `tests::resolve_claude_config_dir_is_deterministic_env_not_scan` | `src/lib.rs:1045` expects suffix `/.cys/claude`. Windows Path::join yields `\.cys\claude`. Test body and helper body are baseline-identical before the isolated-home seam. | Existing separator assertion mismatch. Separately, the original test harness was not actually isolating Windows home; USERPROFILE assignment alone cannot fix that safety defect or the suffix assertion. |
| `tests::runtime_prefixed_path_prepends_exe_dir_and_dedups` | `src/lib.rs:639` requires synthetic current PATH at the end. The existing Windows implementation (`:173–196`) intentionally appends registry and user-bin entries. The CI log prints exactly those appended paths. | Existing stale assertion against registry/user-bin behavior. USERPROFILE may change expanded values, but does not create the contradiction between this assertion and unchanged Windows composition. |

Baseline and M1 Cargo.lock keep identical `dirs 6.0.0` and `dirs-sys 0.5.0` versions/checksums. In those dependencies, Windows dirs::home_dir calls dirs_sys::known_folder_profile, then SHGetKnownFolderPath(FOLDERID_Profile) for the current OS token. HOME and USERPROFILE overrides do not change that API result. The prior CI stopped at the core failures before Windows real-CLI mutation tests; no write by those skipped later CLI steps is claimed.

## Safe test-only seam

`test-home-seam` is a non-default Cargo feature. Only debug/test builds with that feature honor absolute `CYS_HOME`; missing/empty/relative values fail closed. Default and release-profile builds retain the original dirs::home_dir resolution and ignore CYS_HOME. The existing isolated Cargo test runner explicitly enables this feature and sets CYS_HOME to its temporary home. Tauri tests enable the dependency feature through `cys/test-home-seam`.

The CLI integration fixture supplies Korean names/spaces in HOME/USERPROFILE/CYS_HOME and the work folder, then checks the actual child CLI's resolved home before mutating team metadata on Windows. The pure `phoenix-identity` diagnostic has an additive `home_dir` JSON field for this read-only check; it performs no RPC and creates no store/lock. No existing OS profile, registry value or .cys junction is redirected.

CI separately builds a default-feature **release-profile** cys, passes CYS_HOME to it, and checks its output against an independent OS-home query (Windows ctypes SHGetKnownFolderPath; Unix Path.home) while requiring the temporary directory to remain empty. This check does not create a real-profile teams.lock. The check also rejects a feature-enabled debug binary, as observed locally. A release-profile binary has not been built or run locally; the release check belongs to GitHub Actions.

## Same-run baseline proof

The Windows job checks out the exact unchanged baseline into `.ci-baseline`. `scripts/tests/compare_teams_windows_baseline.py` runs the three exact library tests, one at a time, for baseline and candidate with the same toolchain, runner, lockfile policy, temporary HOME/USERPROFILE/CYS_HOME/PACK settings, and a shared Cargo cache. Both comparisons disable default features so the ordinary home path is the same. These three tests either use an explicit temporary pack fixture or only read home/PATH; they do not invoke team mutations or start a daemon.

A known-failure classification requires Cargo exit 101, that exact test reporting FAILED, exactly one executed failed test, and its specific observed assertion marker. Compile/setup failures, zero executed tests, unrelated assertions, or a baseline/candidate difference fail the diagnostic. Each original output and exit code is retained; comparison JSON names the baseline and candidate SHA. Unit tests verify rejection of compilation errors, wrong assertions and vacuous zero-test passes.

The baseline Windows run is still pending. Until it executes, the precise statement is “source-identical pre-existing assertion defects; same-run Windows reproduction wired,” not “baseline Windows measured PASS/FAIL.”

Full current core suites still execute every test. Windows PS7/5.1 steps collect failures while running the remaining CLI, reaper, RPC, Tauri, Python and UI suites, then exit nonzero if any test failed. The PS5.1 step runs even after a PS7 failure. Matching baseline failures do **not** turn the full-suite/release gate green. No continue-on-error, test skip, or assertion weakening is introduced. Theo retains disposition of these known baseline defects.
