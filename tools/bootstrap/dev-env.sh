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
# build/native/sqlite/<target>/ — the target alone, matching build-sqlite.sh.
# Not versioned: the recipe already pins one source and one toolchain, and P1-02-R
# rebuilds on any source/flags/compiler/SDK change rather than reusing by path.
export AVENCREW_SQLITE_DIR="$_DEV_ROOT/build/native/sqlite/$_RUST_HOST"
export AVENCREW_ENV_ACTIVE=1

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

# ---- controlled native SQLite environment ----------------------------------
# Canonical absolute paths into THIS project's native build, plus the settings
# that stop libsqlite3-sys from discovering a host library. build.rs in
# crates/store-sqlite re-validates all of this and fails clearly when absent.
export SQLITE3_INCLUDE_DIR="$AVENCREW_SQLITE_DIR/include"
export SQLITE3_LIB_DIR="$AVENCREW_SQLITE_DIR/lib"
export SQLITE3_STATIC=1
export SQLITE3_NO_PKG_CONFIG=1
export LIBSQLITE3_SYS_USE_PKG_CONFIG=0
export AVENCREW_SQLITE_VERSION="$(python3 "$_PINS" --tool-version sqlite)"

# Binding inputs must come from the same selected developer toolchain as the C
# recipe, never an inherited libclang/header override.
for _override in CPATH C_INCLUDE_PATH CPLUS_INCLUDE_PATH LIBRARY_PATH \
                 LIBSQLITE3_FLAGS LIBSQLITE3_SYS_BUNDLING SQLCIPHER_LIB_DIR \
                 SQLCIPHER_INCLUDE_DIR SQLCIPHER_STATIC; do
  if [ -n "${!_override:-}" ]; then
    echo "dev-env: clear inherited $_override before using the controlled native recipe" >&2
    return 2 2>/dev/null || exit 2
  fi
done
if [ "$(uname -s)" = Darwin ]; then
  _CLANG="$(xcrun --find clang)" || return 2 2>/dev/null || exit 2
  _SDK="$(xcrun --show-sdk-path)" || return 2 2>/dev/null || exit 2
  _LIBCLANG_DIR="$(dirname "$_CLANG")/../lib"
  [ -f "$_LIBCLANG_DIR/libclang.dylib" ] || {
    echo "dev-env: selected developer toolchain has no libclang.dylib" >&2
    return 2 2>/dev/null || exit 2
  }
  export LIBCLANG_PATH="$(cd "$_LIBCLANG_DIR" && pwd)"
  _SQLITE_FLAGS="$(python3 "$_PINS" --field sqlite common_c_flags)" || return 2 2>/dev/null || exit 2
  _MACOS_MIN="$(python3 "$_PINS" --field sqlite macos_min_version)" || return 2 2>/dev/null || exit 2
  export BINDGEN_EXTRA_CLANG_ARGS="--target=$_RUST_HOST -isysroot \"$_SDK\" -mmacosx-version-min=$_MACOS_MIN $_SQLITE_FLAGS"
  # bindgen also accepts target-specific overrides; reject those rather than
  # allowing them to take precedence over the selected arguments above.
  for _override in BINDGEN_EXTRA_CLANG_ARGS_aarch64_apple_darwin; do
    if [ -n "${!_override:-}" ]; then
      echo "dev-env: clear inherited $_override before building" >&2
      return 2 2>/dev/null || exit 2
    fi
  done
fi

export AVENCREW_ENV_VARS="RUSTUP_HOME CARGO_HOME PNPM_HOME AVENCREW_MANAGED_ROOT AVENCREW_TOOLCHAIN AVENCREW_SQLITE_DIR AVENCREW_ENV_ACTIVE AVENCREW_ENV_VARS SQLITE3_INCLUDE_DIR SQLITE3_LIB_DIR SQLITE3_STATIC SQLITE3_NO_PKG_CONFIG LIBSQLITE3_SYS_USE_PKG_CONFIG LIBCLANG_PATH BINDGEN_EXTRA_CLANG_ARGS AVENCREW_SQLITE_VERSION"

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  [ "$#" -gt 0 ] || { echo "usage: dev-env.sh <command> [args...]" >&2; exit 2; }
  exec "$@"
fi
