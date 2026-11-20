# Windows-shaped preparation proof — retry 1

SKILLS_USED: ponytail (lite), tdd, hallucination-guard, debugging-and-error-recovery.
Confidence: High for the local simulated cases; native Windows replay is not measured here.

CI run44686116093 failed all four variants before replay at build_variant.py's changed-path assertion. No scroll-history conclusion can be drawn from that run. Source: saved CI44686116093_FAILED.log / GitHub Actions run44686116093.

The commit includes tests/test_prepare_windows.py. It runs the actual prepare() against retained cloned source trees, feeds PureWindowsPath backslash string semantics into relative_to() while native filesystem access uses __fspath__, uses real CRLF vendor bytes, and places outputs under a Unicode folder. A second scenario simulates cp1252 when no explicit encoding is supplied. Each scenario prepares V1–V4, verifies only the intended copied source changed, exact flags7/7/6/15, original/copy raw hashes, CRLF preservation, Cargo.lock identity and parsed dependency paths. This is eight real preparation invocations under two unittest methods, not native Windows execution.

Before fix, same test on the committed failing builder:4 assertion failures plus4 UnicodeEncodeError cases (retry1/PREP_RED.log). After separator normalization and explicit UTF-8 I/O, the output below passes. Pulse's .as_posix fix is preserved. The Windows child cwd is also explicitly set to the host cwd: vendored cmdbuilder.rs:608–615 otherwise prefers USERPROFILE, which would misplace relative evidence metadata. No app code was changed.

```text
PASS V1 Windows-backslash-relative + CRLF + cp1252-default=True
PASS V2 Windows-backslash-relative + CRLF + cp1252-default=True
PASS V3 Windows-backslash-relative + CRLF + cp1252-default=True
PASS V4 Windows-backslash-relative + CRLF + cp1252-default=True
.PASS V1 Windows-backslash-relative + CRLF + cp1252-default=False
PASS V2 Windows-backslash-relative + CRLF + cp1252-default=False
PASS V3 Windows-backslash-relative + CRLF + cp1252-default=False
PASS V4 Windows-backslash-relative + CRLF + cp1252-default=False
.
----------------------------------------------------------------------
Ran 2 tests in 0.084s

OK
```

Additional local checks: Rust query-parser test1passed (retry1/RUST.log), cargo fmt check passed, judge self-test passed,7PowerShell blocks parsed (retry1/PS_PARSE.log). Full-harness review also checked PS5.1-compatible syntax, PS5 UTF-8-BOM metadata read by pwsh7, SHA256 in the child vs Get-FileHash in the parent, absolute DLL selection, cargo target/debug paths and unchanged binary fixture attributes. CI now executes the prep regression before each actual variant preparation and retains evidence/prep-proof.log.

Limits: simulation does not execute Windows APIs, PS5.1, ConPTY/DLL loading or Windows Cargo compilation; these require the next separately approved CI run. No push or dispatch was performed for this repair.

The workflow pins Python3.12 using the repository's existing setup-python@v5 pattern, so the proof's stdlib tomllib dependency is explicit rather than relying on the runner's default Python. Local proof interpreter was Python3.12.2.
