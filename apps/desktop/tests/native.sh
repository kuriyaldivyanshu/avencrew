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
pnpm --filter @avencrew/desktop build
pnpm --filter @avencrew/desktop test:build
"$ROOT/apps/desktop/node_modules/electron/dist/Electron.app/Contents/MacOS/Electron" "$ROOT/apps/desktop/dist/main/native-test.js" "$FIXTURE" "$ROOT/target/debug/avencrew-supervisor"
export AVENCREW_DESKTOP_FIXTURE_ACTION=verify
cargo test -p avencrew-supervisor --test local_transport --locked desktop_native_fixture -- --exact --ignored --nocapture
