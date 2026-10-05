#!/usr/bin/env bash
# Command-scoped environment for the project-managed development toolchain.
#
# Usage:
#   tools/bootstrap/dev-env.sh <command> [args...]   run one command, then exit
#   source tools/bootstrap/dev-env.sh                activate in the current shell
#
# To leave a sourced activation, restore your own PATH and unset the variables
# this file exports (listed in AVENCREW_ENV_VARS below). There is no `deactivate`
# subcommand; this is a plain shell script, not a shell framework.
#
# It never edits a shell profile, never runs `rustup default`, and never writes
# to ~/.rustup, ~/.cargo or a global package manager. Outside an activation the
# developer's inherited defaults are exactly as they were.

_DEV_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
_PINS="$_DEV_ROOT/tools/bootstrap/pins.py"

if ! command -v python3 >/dev/null 2>&1; then
  echo "dev-env: python3 is required to read tools/bootstrap/pins.json" >&2
  return 2 2>/dev/null || exit 2
fi

_RUST_VERSION="$(python3 "$_PINS" --tool-version rust)" || return 2 2>/dev/null || exit 2
_RUST_HOST="$(python3 "$_PINS" --field rust host)"

export AVENCREW_MANAGED_ROOT="$_DEV_ROOT/build/dev-toolchain"
export RUSTUP_HOME="$AVENCREW_MANAGED_ROOT/rustup"
export CARGO_HOME="$AVENCREW_MANAGED_ROOT/cargo"
export PNPM_HOME="$AVENCREW_MANAGED_ROOT/toolchain/node_modules"
export AVENCREW_TOOLCHAIN="$RUSTUP_HOME/toolchains/${_RUST_VERSION}-${_RUST_HOST}/bin"
export AVENCREW_ENV_ACTIVE=1
export AVENCREW_ENV_VARS="RUSTUP_HOME CARGO_HOME PNPM_HOME AVENCREW_MANAGED_ROOT AVENCREW_TOOLCHAIN AVENCREW_ENV_ACTIVE AVENCREW_ENV_VARS"

if [ ! -d "$AVENCREW_TOOLCHAIN" ]; then
  echo "dev-env: managed toolchain missing at $AVENCREW_TOOLCHAIN" >&2
  echo "        run: bash tools/bootstrap/bootstrap.sh" >&2
  return 2 2>/dev/null || exit 2
fi

# Managed tools first; the caller's inherited PATH remains the fallback.
case ":$PATH:" in
  *":$AVENCREW_TOOLCHAIN:"*) ;;
  *) export PATH="$AVENCREW_TOOLCHAIN:$AVENCREW_MANAGED_ROOT/node/bin:$AVENCREW_MANAGED_ROOT/toolchain/bin:$PATH" ;;
esac

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  [ "$#" -gt 0 ] || { echo "usage: dev-env.sh <command> [args...]" >&2; exit 2; }
  exec "$@"
fi