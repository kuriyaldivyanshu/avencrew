#!/usr/bin/env bash
# SET-00 project-managed development toolchain bootstrap.
#
# Installs the exact pinned versions into build/dev-toolchain/ using
# project-managed Rustup/Cargo homes. Never touches the user's global Rust
# default, shell profile, or any globally installed package manager.
#
# Versions, URLs and digests come from tools/bootstrap/pins.json, which is
# tracked and self-contained. No private design document is required to build.
#
# Re-runnable: every artifact digest is re-verified on each run.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BOOTSTRAP="$ROOT/tools/bootstrap"
PINS="$BOOTSTRAP/pins.py"
MANAGED="$ROOT/build/dev-toolchain"
DL="$MANAGED/downloads"
STAGE="$MANAGED/staging"
LOGS="$MANAGED/logs"
export RUSTUP_HOME="$MANAGED/rustup"
export CARGO_HOME="$MANAGED/cargo"

mkdir -p "$DL" "$STAGE" "$LOGS" "$RUSTUP_HOME" "$CARGO_HOME"
exec > >(tee -a "$LOGS/bootstrap.log") 2>&1

say() { printf '\n=== %s ===\n' "$*"; }
die() { printf 'bootstrap: %s\n' "$*" >&2; exit 2; }

# ---- prerequisites ---------------------------------------------------------
say "prerequisites"
for tool in python3 curl shasum openssl ditto tar uname; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool '$tool' is not on PATH. Install it and re-run; this script does not install system packages."
done
[ -f "$PINS" ] || die "missing $PINS"
[ -f "$BOOTSTRAP/safe_extract.py" ] || die "missing $BOOTSTRAP/safe_extract.py"
echo "prerequisites: ok"

# ---- platform gate ---------------------------------------------------------
if ! python3 "$PINS" --platform; then
  die "unsupported platform (see message above). No substitute version is selected automatically."
fi

RUST_VERSION="$(python3 "$PINS" --tool-version rust)"
NODE_VERSION="$(python3 "$PINS" --tool-version node)"
TC="$RUSTUP_HOME/toolchains/$RUST_VERSION-$(python3 "$PINS" --field rust host)/bin"

# ---- digest helpers --------------------------------------------------------
sha256_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
sha512_b64_of() { openssl dgst -sha512 -binary "$1" | openssl base64 -A; }

expect() { # expect <label> <expected> <actual>
  if [ "$2" = "$3" ]; then
    printf 'OK   %-40s %s\n' "$1" "$3"
  else
    printf 'FAIL %-40s expected=%s actual=%s\n' "$1" "$2" "$3" >&2
    die "integrity mismatch for $1; refusing to continue"
  fi
}

fetch() { curl -fsSL --retry 3 --max-time 900 -o "$2" "$1" || die "download failed: $1"; }

# ---- 1. Rust, via managed RUSTUP_HOME/CARGO_HOME ----------------------------
say "rust $RUST_VERSION"
MANIFEST_URL="$(python3 "$PINS" --field rust channel_manifest_url)"
MANIFEST_SHA="$(python3 "$PINS" --field rust channel_manifest_sha256)"
MANIFEST_BYTES="$(python3 "$PINS" --field rust channel_manifest_bytes)"
MANIFEST_OUT="$DL/channel-rust-$RUST_VERSION.toml"

if [ ! -f "$MANIFEST_OUT" ]; then
  fetch "$MANIFEST_URL" "$MANIFEST_OUT"
fi
expect "rust channel manifest sha256" "$MANIFEST_SHA" "$(sha256_of "$MANIFEST_OUT")"
expect "rust channel manifest bytes" "$MANIFEST_BYTES" "$(wc -c < "$MANIFEST_OUT" | tr -d ' ')"

# Reuse the already-installed rustup *binary*; it only writes inside the managed homes.
RUSTUP_BIN="$(command -v rustup)" || die "rustup not found on PATH. Install Rustup (https://rustup.rs) first; this script manages toolchains, not rustup itself."
echo "rustup binary: $RUSTUP_BIN"

# shellcheck disable=SC2046
"$RUSTUP_BIN" toolchain install "$RUST_VERSION" \
  --profile minimal \
  $(python3 "$PINS" --field rust components | tr ' ' '\n' | sed 's/^/--component /' | tr '\n' ' ') \
  --no-self-update

# ---- 2. Node ---------------------------------------------------------------
say "node $NODE_VERSION"
NODE_TGZ="node-v$NODE_VERSION-darwin-arm64.tar.xz"
fetch "$(python3 "$PINS" --field node archive_url)" "$DL/$NODE_TGZ"
expect "node archive sha256" "$(python3 "$PINS" --field node archive_sha256)" "$(sha256_of "$DL/$NODE_TGZ")"
echo "node archive bytes: $(wc -c < "$DL/$NODE_TGZ" | tr -d ' ')"
rm -rf "$STAGE/node" "$MANAGED/node"
python3 "$BOOTSTRAP/safe_extract.py" "$DL/$NODE_TGZ" tar.xz "$STAGE/node" >/dev/null
mv "$STAGE/node/node-v$NODE_VERSION-darwin-arm64" "$MANAGED/node"
[ -f "$MANAGED/node/LICENSE" ] || die "node LICENSE missing after extraction; refusing an incomplete install"
echo "node license: present"

# ---- 3. npm wrappers + native payloads -------------------------------------
# Direct tarball acquisition with explicit integrity checks, so each native
# payload is verified rather than trusted from wrapper metadata alone.
#
# Layout matters: TypeScript 7 resolves its compiler through
# import.meta.resolve("@typescript/typescript-<platform>-<arch>/package.json"),
# i.e. ordinary Node resolution relative to the wrapper. The wrappers and their
# native packages therefore have to sit in a real node_modules tree, or tsc
# fails with "Unable to resolve @typescript/typescript-darwin-arm64".
NM="$MANAGED/toolchain/node_modules"
TBIN="$MANAGED/toolchain/bin"
mkdir -p "$NM" "$TBIN"

install_npm_pkg() { # install_npm_pkg <tool> <name> <url> <expected-sha512>
  local tool_name="$1" name="$2" url="$3" integrity="$4"
  # pins.json carries native payloads in npm SRI form ("sha512-<b64>") and
  # wrappers as bare base64. Normalise to bare base64 so one comparison covers both.
  integrity="${integrity#sha512-}"
  # Separate statements: `local a=.. b=$a` declares both before assigning either.
  local base="${name##*/}"
  local short="${base}-$(python3 "$PINS" --tool-version "$tool_name").tgz"
  local file="$STAGE/pkg"
  rm -rf "$file"; mkdir -p "$file"
  fetch "$url" "$DL/$short"
  expect "$name sha512" "$integrity" "$(sha512_b64_of "$DL/$short")"
  echo "$name bytes: $(wc -c < "$DL/$short" | tr -d ' ')"
  python3 "$BOOTSTRAP/safe_extract.py" "$DL/$short" tgz "$file" >/dev/null
  # Preserve the npm scope so Node resolution of "@scope/name" works.
  local target="$NM/$name"
  rm -rf "$target"; mkdir -p "$(dirname "$target")"
  mv "$file/package" "$target"
  local got
  got="$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version"])' "$target/package.json")"
  expect "$name version field" "$(python3 "$PINS" --tool-version "$tool_name")" "$got"
}

shim() { printf '#!/usr/bin/env bash\nexec "%s" "$@"\n' "$2" > "$TBIN/$1"; chmod +x "$TBIN/$1"; }

for tool_name in pnpm typescript esbuild; do
  version="$(python3 "$PINS" --tool-version "$tool_name")"
  say "$tool_name $version (wrapper + native payload)"
  install_npm_pkg "$tool_name" "$tool_name" \
    "$(python3 "$PINS" --field "$tool_name" wrapper_url)" \
    "$(python3 "$PINS" --field "$tool_name" wrapper_sha512)"
  install_npm_pkg "$tool_name" \
    "$(python3 "$PINS" --field "$tool_name" native_package)" \
    "$(python3 "$PINS" --field "$tool_name" native_url)" \
    "$(python3 "$PINS" --field "$tool_name" native_sha512)"
done

# The native pnpm ships as a Mach-O executable at the package root (no bin/ field).
shim pnpm "$NM/@pnpm/exe.darwin-arm64/pnpm"
shim tsc "$NM/typescript/bin/tsc"
shim esbuild "$NM/@esbuild/darwin-arm64/bin/esbuild"

# ---- 4. acceptance ---------------------------------------------------------
# Rust components live in the managed RUSTUP_HOME toolchain. cargo resolves
# `cargo fmt`/`cargo clippy` through RUSTUP_HOME, so both must be exported here
# or those subcommands silently fall back to the user's global toolchain.
export PATH="$TC:$MANAGED/node/bin:$TBIN:$PATH"

say "acceptance: managed profile versions"
expect "rustc"    "$RUST_VERSION" "$(rustc --version | awk '{print $2}')"
expect "cargo"    "$RUST_VERSION" "$(cargo --version | awk '{print $2}')"
expect "node"     "v$NODE_VERSION" "$(node --version)"
expect "pnpm"     "$(python3 "$PINS" --tool-version pnpm)"       "$(pnpm --version)"
expect "tsc"      "$(python3 "$PINS" --tool-version typescript)" "$(tsc --version | awk '{print $2}')"
expect "esbuild"  "$(python3 "$PINS" --tool-version esbuild)"    "$(esbuild --version)"
rustfmt --version
cargo clippy --version

say "native payload provenance (must be Darwin arm64 Mach-O)"
file -b "$NM/@pnpm/exe.darwin-arm64/pnpm"
file -b "$NM/@typescript/typescript-darwin-arm64/lib/tsc"
file -b "$NM/@esbuild/darwin-arm64/bin/esbuild"

say "bootstrap complete"
