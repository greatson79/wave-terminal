#!/usr/bin/env bash
# Validate either the staging runtime or Contents/Resources/runtime before distribution.
set -euo pipefail
RT="${1:?runtime directory required}"
TARGET="${2:?target triple required}"
case "$TARGET" in
  aarch64-apple-darwin) ARCH=arm64 ;;
  x86_64-apple-darwin) ARCH=x86_64 ;;
  *) echo "unsupported mac target: $TARGET" >&2; exit 1 ;;
esac
for rel in python/bin/python3 git/bin/git uv/uv uv/uvx node/bin/node node/bin/npm node/bin/npx; do
  if [ ! -x "$RT/$rel" ]; then
    echo "missing executable runtime: $RT/$rel" >&2; exit 1
  fi
done
for rel in python/bin/python3 git/bin/git uv/uv uv/uvx node/bin/node; do
  /usr/bin/lipo "$RT/$rel" -verify_arch "$ARCH"
done
# Native target: prove Python's required extensions and the tools work without Homebrew/CLT PATH.
# Cross target: architecture/layout only; the target machine must run this same gate natively.
if [ "$(uname -m)" = "$ARCH" ]; then
  RT="$(cd "$RT" && pwd)"
  export PATH="$RT/python/bin:$RT/git/bin:$RT/uv:$RT/node/bin:/usr/bin:/bin:/usr/sbin:/sbin"
  export PYTHONDONTWRITEBYTECODE=1
  "$RT/python/bin/python3" -I -c 'import ssl, sqlite3, ctypes, venv, json; print("bundled Python imports OK")'
  "$RT/python/bin/python3" -I -m pip --version
  "$RT/git/bin/git" --version
  "$RT/uv/uv" --version
  "$RT/uv/uvx" --version
  "$RT/node/bin/node" --version
  "$RT/node/bin/npm" --version
  "$RT/node/bin/npx" --version
else
  echo "Cross target: native execution pending ($TARGET)"
fi
echo "macOS runtime validation passed: $RT ($TARGET)"
