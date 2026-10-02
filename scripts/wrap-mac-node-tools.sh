#!/usr/bin/env bash
# Tauri dereferences resource symlinks; preserve each CLI's module-relative entrypoint.
set -euo pipefail
NODE_ROOT="${1:?node runtime directory required}"
[ -x "$NODE_ROOT/bin/node" ] || { echo 'missing bundled node' >&2; exit 1; }
TOOLS=(npm npx corepack)
TARGETS=(../lib/node_modules/npm/bin/npm-cli.js ../lib/node_modules/npm/bin/npx-cli.js ../lib/node_modules/corepack/dist/corepack.js)
# Validate the complete layout before replacing any entrypoint.
for target in "${TARGETS[@]}"; do
  [ -f "$NODE_ROOT/bin/$target" ] || { echo "missing node CLI target: $target" >&2; exit 1; }
done
TEMP_WRAPPER=''
trap '[ -z "$TEMP_WRAPPER" ] || rm -f "$TEMP_WRAPPER"' EXIT
for index in "${!TOOLS[@]}"; do
  tool="${TOOLS[$index]}"
  target="${TARGETS[$index]}"
  TEMP_WRAPPER="$(mktemp "$NODE_ROOT/bin/.${tool}.XXXXXX")"
  cat > "$TEMP_WRAPPER" <<'WRAPPER'
#!/bin/sh
BIN_DIR="$(CDPATH= cd -P "$(dirname "$0")" && pwd)" || exit 1
WRAPPER
  printf 'exec "$BIN_DIR/node" "$BIN_DIR/%s" "$@"\n' "$target" >> "$TEMP_WRAPPER"
  chmod 755 "$TEMP_WRAPPER"
  # Rename replaces the link itself. Redirecting into bin/npm would overwrite npm-cli.js.
  mv -f "$TEMP_WRAPPER" "$NODE_ROOT/bin/$tool"
  TEMP_WRAPPER=''
done
