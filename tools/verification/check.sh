#!/usr/bin/env bash
# Existing scaffold checks, shared by local development and CI.
# Bootstrap/install first; this script never installs or chooses substitute pins.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LANE="${1:-all}"
if [ "$#" -gt 1 ]; then
  echo 'usage: bash tools/verification/check.sh [rust|desktop|contracts|fixtures|licenses|all]' >&2
  exit 2
fi
case "$LANE" in
  rust|desktop|contracts|fixtures|licenses|all) ;;
  *) echo "check: unknown lane '$LANE'" >&2; exit 2 ;;
esac
case "$(uname -s)/$(uname -m)" in
  Darwin/arm64|Linux/aarch64) ;;
  *) echo 'check: UNVERIFIED profile; no substitute is admitted' >&2; exit 2 ;;
esac
# shellcheck source=../bootstrap/dev-env.sh
source "$ROOT/tools/bootstrap/dev-env.sh"
cd "$ROOT"

rust_checks() {
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  cargo test --workspace --all-features --locked
  cargo build --workspace --locked
  cargo run -p avencrew-store-sqlite --example engine_check --locked
  local output binary links
  output="$(cargo metadata --no-deps --locked --format-version 1 | python3 -c 'import sys,json; print(json.load(sys.stdin)["target_directory"])')"
  binary="$output/debug/examples/engine_check"
  if [ "$(uname -s)" = Darwin ]; then
    links="$(otool -L "$binary")"
    nm -gU "$SQLITE3_LIB_DIR/libsqlite3.a" > "$output/sqlite-symbols.txt"
  else
    links="$(ldd "$binary")"
    nm -g --defined-only "$SQLITE3_LIB_DIR/libsqlite3.a" > "$output/sqlite-symbols.txt"
  fi
  printf '%s\n' "$links"
  if printf '%s\n' "$links" | grep -i 'libsqlite'; then
    echo 'check: unexpected dynamic SQLite linkage' >&2; exit 1
  fi
  grep -Eq ' [TW] _?sqlite3_table_column_metadata$' "$output/sqlite-symbols.txt"
  if grep -Eq ' _?sqlite3_(load_extension|enable_load_extension)$' "$output/sqlite-symbols.txt"; then
    echo 'check: extension loading symbols must be absent' >&2; exit 1
  fi
}
desktop_checks() {
  pnpm format:check
  pnpm lint
  pnpm typecheck
  pnpm build
}
contract_checks() {
  pnpm --filter @avencrew/contracts-ts check
}
fixture_checks() {
  # The fixture launcher checks the same Rust source corpus offline. Its data
  # root is throwaway, not application storage; it does not start Electron.
  bash tools/dev/dev.sh
}
license_checks() {
  python3 tools/verification/licenses.py
}
case "$LANE" in
  rust) rust_checks ;;
  desktop) desktop_checks ;;
  contracts) contract_checks ;;
  fixtures) fixture_checks ;;
  licenses) license_checks ;;
  all) rust_checks; desktop_checks; contract_checks; fixture_checks; license_checks ;;
esac
