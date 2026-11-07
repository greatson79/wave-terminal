# Security Policy

## Reporting a vulnerability

Report security issues through [Wave Terminal GitHub Issues](https://github.com/greatson79/wave-terminal/issues).
Do not include credentials, personal information, or exploit details in a public issue;
ask for a private reporting channel first.

## Scope notes

- The desktop app talks to a local daemon (`cysd`) over a user-owned Unix socket
  (macOS/Linux) or a DACL-sealed named pipe (Windows). No network listener is opened.
- Updates are verified: app binaries via Tauri updater signatures, packs via
  minisign (`pack-manifest.json.minisig`, key pinned in the binary).
- External URL opening is gated by a hard host allowlist
  (`~/.cys/url-allow-hosts` / `CYS_URL_ALLOW_HOSTS` extend it locally only).
- A pre-publish secret/PII gate exists at `scripts/secret-scan.sh --all`
  (fail-closed; static pattern matching — see its header for honest limits).
