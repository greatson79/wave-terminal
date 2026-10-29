# Release artifact path gate

CI scans the macOS `.app` tar and DMG, plus the extracted Windows NSIS payload. Extraction or unreadable-file errors fail closed.

- **Private identifiers and local paths:** owner identifier `kylechoi` and its macOS home path are forbidden; any match fails the gate.
- **CI build paths:** the runner home and checkout root are counted and reported, but do not fail the gate. These paths can occur in upstream prebuilt runtime assets such as dugite-native.
- **Evidence:** CI reports private and CI-path file counts separately, plus relative bundle components for matches. Upstream assets are named with counts and a sample filename; do not strip or patch them to hide their paths.

Only CI-built release artifacts count as gate evidence. Do not run the app, `cys`, `cysd`, or an installer on the developer Mac to establish a pass. No tag or release is part of this gate.
