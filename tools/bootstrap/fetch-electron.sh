#!/usr/bin/env bash
# Fetch the exact Electron Darwin ARM64 archive and verify it before extraction.
#
# Electron's npm package normally downloads this binary from a postinstall script.
# That script is denied in this workspace (pnpm-workspace.yaml -> allowBuilds), so
# the binary is acquired here instead and checked against the digest in
# tools/bootstrap/pins.json BEFORE anything is unpacked.
#
# No version is resolved by guesswork and no digest is recomputed from our own
# download: the expected value comes from the tracked pin file, so a clean
# checkout needs no private design document.

set -euo pipefail
[ "$(uname -s)/$(uname -m)" = Darwin/arm64 ] || { echo "fetch-electron: desktop distribution supports macOS ARM64 only" >&2; exit 2; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BOOTSTRAP="$ROOT/tools/bootstrap"
PINS="$BOOTSTRAP/pins.py"
MANAGED="$ROOT/build/dev-toolchain"
DL="$MANAGED/downloads"
STAGE="$MANAGED/staging"
LOGS="$MANAGED/logs"
mkdir -p "$DL" "$STAGE" "$LOGS"

ELECTRON_VERSION="$(python3 "$PINS" --tool-version electron)"
ELECTRON_DIR="$ROOT/apps/desktop/node_modules/electron"

# Clear prerequisites error rather than a confusing downstream failure.
for tool in python3 curl shasum ditto; do
  command -v "$tool" >/dev/null 2>&1 || { printf 'fetch-electron: required tool %s is not on PATH\n' "$tool" >&2; exit 2; }
done
python3 "$PINS" --platform || { printf 'fetch-electron: unsupported platform\n' >&2; exit 2; }
[ -d "$ELECTRON_DIR" ] || { printf 'fetch-electron: %s does not exist. Run `pnpm install` first.\n' "$ELECTRON_DIR" >&2; exit 2; }

BINARY_URL="$(python3 "$PINS" --field electron binary_url)"
EXPECTED_SHA="$(python3 "$PINS" --field electron binary_sha256)"
ZIP="$DL/electron-v$ELECTRON_VERSION-darwin-arm64.zip"
LOG="$LOGS/electron-binary.log"
exec > >(tee "$LOG") 2>&1

echo "electron version: $ELECTRON_VERSION"
echo "expected sha256 : $EXPECTED_SHA"
echo "source          : $BINARY_URL"

# Reuse an already-downloaded, already-verified archive.
if [ -f "$ZIP" ] && [ "$(shasum -a 256 "$ZIP" | cut -d' ' -f1)" = "$EXPECTED_SHA" ]; then
  echo "cache           : reusing verified archive"
else
  curl -fsSL --retry 3 --max-time 1800 -o "$ZIP" "$BINARY_URL" || { echo "download failed" >&2; exit 2; }
fi

ACTUAL_SHA="$(shasum -a 256 "$ZIP" | cut -d' ' -f1)"
echo "actual   sha256 : $ACTUAL_SHA"
if [ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]; then
  echo "INTEGRITY BLOCKER: Electron archive digest does not match pins.json." >&2
  exit 1
fi
echo "digest   match  : OK"
echo "archive  bytes  : $(wc -c < "$ZIP" | tr -d ' ')"

rm -rf "$STAGE/electron"
mkdir -p "$STAGE/electron"
ditto -x -k "$ZIP" "$STAGE/electron"

# Containment check after extraction: no symlink may point outside the staging root.
python3 - "$STAGE/electron" <<'PY'
import os, sys
root = os.path.realpath(sys.argv[1])
bad = []
for base, dirs, files in os.walk(root):
    for name in dirs + files:
        path = os.path.join(base, name)
        if os.path.islink(path):
            target = os.path.realpath(path)
            if not (target == root or target.startswith(root + os.sep)):
                bad.append((os.path.relpath(path, root), target))
if bad:
    for rel, target in bad:
        print(f'ESCAPING LINK: {rel} -> {target}', file=sys.stderr)
    sys.exit(1)
print('containment check: no symlink escapes the staging root')
PY

if [ ! -d "$STAGE/electron/Electron.app" ]; then
  echo "LAYOUT BLOCKER: expected Electron.app at the archive root." >&2
  exit 1
fi

# Place the verified tree in the layout electron/index.js expects:
# it resolves `path.join(__dirname, 'dist', <path.txt contents>)`.
# The destination directory must exist FIRST, otherwise `mv` renames the
# application to `dist` instead of creating `dist/Electron.app`, and the launcher
# then silently falls back to downloading its own binary.
#
# Move the whole archive root, not just the bundle: upstream ships LICENSE,
# LICENSES.chromium.html and `version` beside Electron.app, and reproducing only
# the bundle leaves dist/ diverging from the real distribution.
rm -rf "$ELECTRON_DIR/dist"
mkdir -p "$ELECTRON_DIR/dist"
for entry in "$STAGE"/electron/*; do
  mv "$entry" "$ELECTRON_DIR/dist/"
done
echo "dist entries   : $(ls "$ELECTRON_DIR/dist" | tr '\n' ' ')"

# electron/index.js resolves `path.join(__dirname, 'dist', <path.txt contents>)`
# and cli.js spawns exactly that. For darwin the spawnable path is the inner
# binary, NOT the .app bundle: spawning a directory fails with EACCES.
#
# This string is electron's own `platformPath` (install.js, getPlatformPath),
# and install.js compares path.txt to it byte-for-byte, so there must be no
# trailing newline. Getting this wrong is silent until launch time.
PLATFORM_PATH='Electron.app/Contents/MacOS/Electron'
printf '%s' "$PLATFORM_PATH" > "$ELECTRON_DIR/path.txt"

if [ ! -x "$ELECTRON_DIR/dist/$PLATFORM_PATH" ]; then
  echo "LAYOUT BLOCKER: dist/$PLATFORM_PATH is missing or not executable." >&2
  exit 1
fi

# install.js also compares dist/version against the package version. If that
# disagrees, electron would consider itself uninstalled and re-download.
if [ ! -f "$ELECTRON_DIR/dist/version" ]; then
  echo "LAYOUT BLOCKER: dist/version missing; the archive root was not fully installed." >&2
  exit 1
fi
INSTALLED_VERSION="$(tr -d 'v' < "$ELECTRON_DIR/dist/version" | tr -d '[:space:]')"
if [ "$INSTALLED_VERSION" != "$ELECTRON_VERSION" ]; then
  echo "VERSION BLOCKER: dist/version is $INSTALLED_VERSION, expected $ELECTRON_VERSION." >&2
  exit 1
fi
echo "path.txt       : $(cat "$ELECTRON_DIR/path.txt")"
echo "dist/version   : $(cat "$ELECTRON_DIR/dist/version")  (matches pin $ELECTRON_VERSION)"

echo "installed       : $ELECTRON_DIR/dist/Electron.app"
file -b "$ELECTRON_DIR/dist/Electron.app/Contents/MacOS/Electron"
echo "NOTE: digest verified against pins.json. NO publisher signature was checked."