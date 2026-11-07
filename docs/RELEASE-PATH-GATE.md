# Release artifact path gate

CI scans the macOS `.app` tar and DMG, plus the extracted Windows NSIS payload. Extraction or unreadable-file errors fail closed.

- **Private identifiers and local paths:** owner identifier `kylechoi` and its macOS home path are forbidden; any match fails the gate.
- **CI build paths:** the runner home and checkout root are counted and reported, but do not fail the gate. These paths can occur in upstream prebuilt runtime assets such as dugite-native.
- **Evidence:** CI reports private and CI-path file counts separately, plus relative bundle components for matches. Upstream assets are named with counts and a sample filename; do not strip or patch them to hide their paths.

Only CI-built release artifacts count as gate evidence. Do not run the app, `cys`, `cysd`, or an installer on the developer Mac to establish a pass. No tag or release is part of this gate.


## Install and contact source references


Install, manual update, download and homepage guidance belongs at
https://waveainetworks.com/get. Contact: https://github.com/greatson79/wave-terminal/issues.
Machine update manifests and signed asset URLs keep their functional endpoints.

Run `python3 scripts/check-release-links.py` from the checkout root. Release, Windows
and macOS build workflows run it before building. The gate performs a case-insensitive
Git search of tracked source for the upstream account identity. Only exact line hashes
in `scripts/release-path-allowlist.json` are accepted; each file has a reason:

- `LICENSE`, `LICENSES/cys-terminal-MIT.txt`: mandatory original MIT copyright.
- `cysjavis-pack/bin/javis_org.py`: synthetic account keys in `self_test`.
- `cysjavis-pack/bin/javis_org_e2e.sh`: synthetic account keys in isolated test data.

Any unapproved line content or location, changed allowed line, or Git error fails the gate. A fixture
change requires an intentional allowlist update and review. This is a regression check
for the known upstream identity, not a general detector of every incorrect URL.

Run `python3 scripts/tests/test_release_links.py` for the negative controls: a changed
license line with a download URL and an uppercase unapproved identity must fail. An
exact license line passes; a failed Git scan must never count as clean. User dialog
and host-allowlist tests cover the actual download target separately.
