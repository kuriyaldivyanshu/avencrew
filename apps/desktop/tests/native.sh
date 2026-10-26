#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
if [ "$(uname -s)/$(uname -m)" != Darwin/arm64 ]; then
  echo 'desktop-native: UNVERIFIED on this OS; supported execution lane is macOS ARM64' >&2
  exit 2
fi
source "$ROOT/tools/bootstrap/dev-env.sh"
cd "$ROOT"
FIXTURE="$(mktemp -d /tmp/av-desktop-XXXXXXXX)"
FIXTURE="$(cd "$FIXTURE" && pwd -P)"
# Owned synthetic fixture only. Ordinary window close never kills the supervisor.
trap 'rm -rf "$FIXTURE"' EXIT
export AVENCREW_DESKTOP_FIXTURE_ROOT="$FIXTURE"
export AVENCREW_DESKTOP_FIXTURE_ACTION=seed
cargo test -p avencrew-supervisor --test local_transport --locked desktop_native_fixture -- --exact --ignored --nocapture
CARGO_TARGET="$(cargo metadata --no-deps --locked --format-version 1 | python3 -c 'import sys,json; print(json.load(sys.stdin)["target_directory"])')"
printf 'desktop-native cargo target: %s\n' "$CARGO_TARGET"
pnpm --filter @avencrew/desktop build
pnpm --filter @avencrew/desktop test:build
"$ROOT/apps/desktop/node_modules/electron/dist/Electron.app/Contents/MacOS/Electron" "$ROOT/apps/desktop/dist/main/native-test.js" "$FIXTURE" "$CARGO_TARGET/debug/avencrew-supervisor"
mkdir "$FIXTURE/moved"
cp -R "$ROOT/apps/desktop/dist" "$FIXTURE/moved/dist"
printf '%s\n' '{"type":"module"}' > "$FIXTURE/moved/package.json"
"$ROOT/apps/desktop/node_modules/electron/dist/Electron.app/Contents/MacOS/Electron" "$FIXTURE/moved/dist/main/relocation-test.js" "$FIXTURE"
export AVENCREW_DESKTOP_FIXTURE_ACTION=verify
cargo test -p avencrew-supervisor --test local_transport --locked desktop_native_fixture -- --exact --ignored --nocapture
