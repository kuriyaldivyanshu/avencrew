#!/usr/bin/env bash
# Build the pinned SQLite amalgamation into a private static archive.
#
# Implements the controlled native recipe: verify the archive digest, extract
# safely, confirm header identity, then compile the UNMODIFIED amalgamation with
# the exact reviewed C flags. Outputs live under build/native/sqlite/<target>/
# and are never checked in.
#
# Everything is read from tools/bootstrap/pins.json. No host SQLite, no Homebrew
# SQLite, no Cargo source patch, no download from build.rs.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BOOTSTRAP="$ROOT/tools/bootstrap"
PINS="$BOOTSTRAP/pins.py"
NATIVE="$ROOT/build/native"
DL="$ROOT/build/dev-toolchain/downloads"
STAGE="$NATIVE/staging"
LOGS="$ROOT/build/dev-toolchain/logs"
mkdir -p "$NATIVE" "$DL" "$STAGE" "$LOGS"

say() { printf '\n=== %s ===\n' "$*"; }
die() { printf 'build-sqlite: %s\n' "$*" >&2; exit 2; }

SQLITE_VERSION="$(python3 "$PINS" --tool-version sqlite)"
SQLITE_SOURCE_ID="$(python3 "$PINS" --field sqlite source_id)"
SQLITE_SHA3="$(python3 "$PINS" --field sqlite amalgamation_sha3_256)"
SQLITE_URL="$(python3 "$PINS" --field sqlite amalgamation_url)"
SQLITE_FLAGS="$(python3 "$PINS" --field sqlite common_c_flags)"
MACOS_MIN="$(python3 "$PINS" --field sqlite macos_min_version)"

# ---- prerequisites ---------------------------------------------------------
say "prerequisites"
for tool in python3 curl unzip xcrun ar; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool '$tool' is not on PATH"
done
[ -f "$PINS" ] || die "missing $PINS"

# ---- platform profile ------------------------------------------------------
say "platform profile"
KERNEL="$(uname -s)/$(uname -m)"
case "$KERNEL" in
  Darwin/arm64) PROFILE=macos-arm64 ;;
  Linux/aarch64|Linux/arm64) PROFILE=linux-arm64 ;;
  *) die "no SQLite recipe for $KERNEL. This recipe covers macos-arm64 and linux-arm64 only; no substitute is selected automatically." ;;
esac
TARGET="$(python3 "$PINS" --field rust host)"
echo "kernel : $KERNEL"
echo "profile: $PROFILE"
echo "target : $TARGET"

case "$PROFILE" in
  macos-arm64)
    [ "$TARGET" = "aarch64-apple-darwin" ] || die "profile $PROFILE requires rust host aarch64-apple-darwin, got $TARGET"
    CLANG="$(xcrun --find clang)" || die "xcrun --find clang failed; install the Xcode Command Line Tools"
    LIBTOOL="$(xcrun --find libtool)" || die "xcrun --find libtool failed"
    SDK="$(xcrun --show-sdk-path)" || die "xcrun --show-sdk-path failed"
    [ -d "$SDK" ] || die "SDK path does not exist: $SDK"
    # libclang must be a real file for bindgen, not just a clang executable.
    LIBCLANG_PATH="$(cd "$(dirname "$CLANG")/../lib" && pwd)"
    [ -n "$LIBCLANG_PATH" ] || die "libclang.dylib not found. bindgen needs a real libclang; do not let build.rs fall back to any other source."
    CLANG_VERSION="$("$CLANG" --version | head -1)"
    SDK_VERSION="$(xcrun --show-sdk-version)"
    ARCHIVE_CMD="xcrun libtool -static"
    TARGET_ARGS=(-arch arm64 "-mmacosx-version-min=$MACOS_MIN" -isysroot "$SDK")
    TARGET_FLAGS="${TARGET_ARGS[*]}"
    LINK_PROBE=(otool -L)
    ;;
  linux-arm64)
    [ "$TARGET" = "aarch64-unknown-linux-gnu" ] || die "profile $PROFILE requires rust host aarch64-unknown-linux-gnu, got $TARGET"
    command -v cc >/dev/null 2>&1 || die "no C compiler (cc) on PATH"
    command -v ar >/dev/null 2>&1 || die "no ar on PATH"
    CLANG="$(command -v cc)"
    LIBCLANG_PATH=""
    for candidate in /usr/lib/llvm-*/lib /usr/lib64/llvm/lib /usr/lib/x86_64-linux-gnu; do
      [ -f "$candidate/libclang.so" ] && { LIBCLANG_PATH="$(cd "$candidate" && pwd)"; break; }
    done
    [ -n "$LIBCLANG_PATH" ] || die "libclang.so not found; bindgen needs a real libclang"
    CLANG_VERSION="$("$CLANG" --version | head -1)"
    SDK_VERSION="n/a (native linux)"
    ARCHIVE_CMD="ar rcs"
    TARGET_ARGS=()
    TARGET_FLAGS=""
    LINK_PROBE=(ldd)
    ;;
esac
echo "clang    : $CLANG_VERSION"
echo "sdk      : $SDK_VERSION  (${SDK:-native-linux})"
echo "libclang : $LIBCLANG_PATH/libclang.$( [ "$PROFILE" = macos-arm64 ] && echo dylib || echo so )"
echo "archive  : $ARCHIVE_CMD"

# ---- acquire and verify ----------------------------------------------------
say "acquire and verify"
ZIP="$DL/sqlite-amalgamation-$SQLITE_VERSION.zip"
if [ ! -f "$ZIP" ]; then
  curl -fsSL --retry 3 --max-time 600 -o "$ZIP" "$SQLITE_URL" || die "download failed: $SQLITE_URL"
fi
# SHA3-256 via python hashlib: macOS `shasum -a` has no sha3, and system openssl
# is LibreSSL, which also lacks it.
ACTUAL_SHA3="$(python3 -c 'import hashlib,sys;print(hashlib.sha3_256(open(sys.argv[1],"rb").read()).hexdigest())' "$ZIP")"
echo "expected sha3-256: $SQLITE_SHA3"
echo "actual   sha3-256: $ACTUAL_SHA3"
if [ "$ACTUAL_SHA3" != "$SQLITE_SHA3" ]; then
  die "integrity mismatch for the SQLite amalgamation; refusing to build"
fi
echo "digest match     : OK"
echo "archive bytes    : $(wc -c < "$ZIP" | tr -d ' ')"

# ---- extract and check header identity -------------------------------------
say "extract and header identity"
rm -rf "$STAGE/sqlite"
python3 "$BOOTSTRAP/safe_extract.py" "$ZIP" zip "$STAGE/sqlite" >/dev/null || die "safe extraction rejected the archive"
SRC_DIR=""
for candidate in "$STAGE"/sqlite/*/; do
  if [ -f "${candidate}sqlite3.c" ] && [ -f "${candidate}sqlite3.h" ]; then
    SRC_DIR="${candidate%/}"
    break
  fi
done
[ -n "$SRC_DIR" ] || die "sqlite3.c/sqlite3.h not found in the extracted archive"
echo "source dir : $SRC_DIR"

HEADER_VERSION="$(sed -n 's/^#define SQLITE_VERSION *"\([^"]*\)".*/\1/p' "$SRC_DIR/sqlite3.h" | head -1)"
HEADER_NUMBER="$(sed -n 's/^#define SQLITE_VERSION_NUMBER *\([0-9]*\).*/\1/p' "$SRC_DIR/sqlite3.h" | head -1)"
echo "header SQLITE_VERSION        : $HEADER_VERSION"
echo "header SQLITE_VERSION_NUMBER : $HEADER_NUMBER"
[ "$HEADER_VERSION" = "$SQLITE_VERSION" ] || die "header version $HEADER_VERSION != pinned $SQLITE_VERSION"
[ "$HEADER_NUMBER" = "$(python3 "$PINS" --field sqlite version_number)" ] || die "header version number $HEADER_NUMBER != pinned"

# ---- compile ---------------------------------------------------------------
say "compile"
OUT="$NATIVE/sqlite/$TARGET"
rm -rf "$OUT"
mkdir -p "$OUT/include" "$OUT/lib" "$OUT/obj" "$OUT/src"
cp "$SRC_DIR/sqlite3.h" "$SRC_DIR/sqlite3.c" "$OUT/src/"
cp "$SRC_DIR/sqlite3.h" "$OUT/include/"

# Retain provenance and the distribution notice. SQLite is public domain and
# carries its blessing in the source header rather than a separate LICENSE file,
# so copy the header's notice block explicitly instead of inventing one.
cat > "$OUT/LICENSE.sqlite" <<EOF
SQLite $SQLITE_VERSION — public domain.

The author disclaims copyright to this source code. In place of a legal notice,
here is a blessing:

    May you do good and not evil.
    May you find forgiveness for yourself and forgive others.
    May you share freely, never taking more than you give.

Source ID : $SQLITE_SOURCE_ID
Amalgamation SHA3-256 : $SQLITE_SHA3
Origin    : $SQLITE_URL

Full notice text is retained verbatim in include/sqlite3.h and src/sqlite3.c.
EOF
cp "$SRC_DIR/shell.c" "$SRC_DIR/sqlite3ext.h" "$OUT/src/" 2>/dev/null || true

echo "cflags: $SQLITE_FLAGS $TARGET_FLAGS"
# shellcheck disable=SC2086
"$CLANG" $SQLITE_FLAGS "${TARGET_ARGS[@]}" -c "$OUT/src/sqlite3.c" -o "$OUT/obj/sqlite3.o" \
  || die "compilation failed"

say "archive"
# The output path is only known here, so the archiver command is assembled per
# profile rather than up front.
case "$PROFILE" in
  macos-arm64)
    "$LIBTOOL" -static -o "$OUT/lib/libsqlite3.a" "$OUT/obj/sqlite3.o" || die "archiving failed"
    ;;
  linux-arm64)
    ar rcs "$OUT/lib/libsqlite3.a" "$OUT/obj/sqlite3.o" || die "archiving failed"
    ;;
esac
[ -f "$OUT/lib/libsqlite3.a" ] || die "libsqlite3.a was not produced"
echo "archive : $OUT/lib/libsqlite3.a ($(wc -c < "$OUT/lib/libsqlite3.a" | tr -d ' ') bytes)"
echo "header  : $OUT/include/sqlite3.h"
echo "source  : $OUT/src/sqlite3.c"
echo "profile : $PROFILE  target=$TARGET"

cat > "$OUT/build-info.txt" <<EOF
profile=$PROFILE
target=$TARGET
sqlite_version=$SQLITE_VERSION
sqlite_version_number=$(python3 "$PINS" --field sqlite version_number)
sqlite_source_id=$SQLITE_SOURCE_ID
amalgamation_sha3_256=$SQLITE_SHA3
clang=$CLANG_VERSION
sdk=$SDK_VERSION
libclang=$LIBCLANG_PATH
cflags=$SQLITE_FLAGS
target_flags=$TARGET_FLAGS
EOF
echo "build info: $OUT/build-info.txt"

say "build complete"
echo "Source this profile before a Cargo build:"
echo "  tools/bootstrap/dev-env.sh cargo build --workspace --locked"