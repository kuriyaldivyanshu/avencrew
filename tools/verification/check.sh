#!/usr/bin/env bash
# Existing scaffold checks, shared by local development and CI.
# Bootstrap/install first; this script never installs or chooses substitute pins.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LANE="${1:-all}"
if [ "$#" -gt 1 ]; then
  echo 'usage: bash tools/verification/check.sh [rust|desktop|contracts|fixtures|all]' >&2
  exit 2
fi
case "$LANE" in
  rust|desktop|contracts|fixtures|all) ;;
  *) echo "check: unknown lane '$LANE'" >&2; exit 2 ;;
esac
if [ "$(uname -s)/$(uname -m)" != Darwin/arm64 ]; then
  echo 'check: UNVERIFIED profile; only macOS ARM64 is admitted by the current pins' >&2
  exit 2
fi
# shellcheck source=../bootstrap/dev-env.sh
source "$ROOT/tools/bootstrap/dev-env.sh"
cd "$ROOT"

rust_checks() {
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  cargo test --workspace --all-features --locked
  cargo build --workspace --locked
  cargo run -p avencrew-store-sqlite --example engine_check --locked
}
desktop_checks() {
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
case "$LANE" in
  rust) rust_checks ;;
  desktop) desktop_checks ;;
  contracts) contract_checks ;;
  fixtures) fixture_checks ;;
  all) rust_checks; desktop_checks; contract_checks; fixture_checks ;;
esac
