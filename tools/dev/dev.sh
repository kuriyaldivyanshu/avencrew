#!/usr/bin/env bash
# Fixture-only development launcher.
#
# One command. After `bash tools/bootstrap/bootstrap.sh` has run once, this
# needs no API key, no network and no paid service:
#
#   bash tools/dev/dev.sh
#
# It seeds an isolated temporary data root from committed synthetic fixtures,
# verifies the fixtures against the canonical Rust decoders, and reports where
# everything landed. It does not create a database, run a model, start a
# supervisor, or claim any work succeeded.
#
# Options:
#   --data-root DIR   Seed into DIR instead of a fresh temporary root.
#                     Must still satisfy the safety rules in seed.mjs.
#   --keep            Do not print the throwaway-command hint.
#   --no-verify       Skip the Rust fixture verification step.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEV_ENV="$ROOT/tools/bootstrap/dev-env.sh"

[ -f "$DEV_ENV" ] || { echo "dev: missing tools/bootstrap/dev-env.sh; run tools/bootstrap/bootstrap.sh first" >&2; exit 2; }

# shellcheck source=/dev/null
source "$DEV_ENV"
cd "$ROOT"

DATA_ROOT=""
KEEP=0
VERIFY=1

while [ $# -gt 0 ]; do
  case "$1" in
    --data-root)
      [ $# -ge 2 ] || { echo "dev: --data-root needs a directory" >&2; exit 2; }
      DATA_ROOT="$2"; shift 2 ;;
    --keep) KEEP=1; shift ;;
    --no-verify) VERIFY=0; shift ;;
    -h|--help) sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "dev: unknown option $1" >&2; exit 2 ;;
  esac
done

# --- 1. seed an isolated root ------------------------------------------------
if [ -n "$DATA_ROOT" ]; then
  SEED_OUT="$(node "$ROOT/tools/dev/seed.mjs" --data-root "$DATA_ROOT" --print-root)"
else
  SEED_OUT="$(node "$ROOT/tools/dev/seed.mjs" --print-root)"
fi
DATA_ROOT_RESOLVED="$SEED_OUT"

echo "data root: $DATA_ROOT_RESOLVED"
echo

# --- 2. verify the fixtures against the canonical Rust decoders --------------
if [ "$VERIFY" -eq 1 ]; then
  echo "verifying fixtures against Rust..."
  # The contract corpus is validated by the crate's own conformance tests, which
  # read the canonical Rust fixture file. This verifies that source corpus;
  # seed hashes describe copied bytes, not business-scenario acceptance.
  if ! cargo test -q -p avencrew-contracts --all-features --locked --offline --tests >/dev/null 2>&1; then
    echo "dev: fixture verification failed; the seeded root is NOT trustworthy" >&2
    cargo test -p avencrew-contracts --all-features --locked --offline --tests 2>&1 | tail -20 >&2
    exit 1
  fi
  echo "  ok: Rust conformance tests pass against the committed fixture corpus"
fi

echo
echo "seeded files:"
(cd "$DATA_ROOT_RESOLVED" && find . -type f | sort)
echo

if [ "$KEEP" -eq 0 ]; then
  echo "this root is temporary and can be deleted at any time:"
  printf '  rm -rf -- %q\n' "$DATA_ROOT_RESOLVED"
fi

cat <<'EOF'
no database was created, no model ran, no network was used, and nothing here
is evidence that any task or run succeeded. For real behaviour oracles see
docs/implementation/fixtures/CROSS_FUNCTIONAL_ORACLES.md (P0-05, bytes P3-01).
EOF
