# tools/bootstrap — project-managed development toolchain

Project-managed installation of the **selected** development tool versions, kept
entirely under `build/dev-toolchain/` (already git-ignored). Nothing here touches
your global configuration.

## What gets installed

| Tool | Version | How |
|---|---|---|
| rustc / cargo | 1.99.0 | existing `rustup` binary, project-managed `RUSTUP_HOME`/`CARGO_HOME` |
| rustfmt | 1.10.0-stable (component of 1.99.0) | `--component rustfmt` |
| clippy | 0.1.99 (component of 1.99.0) | `--component clippy` |
| Node | 24.21.0 | official ARM64 archive for the selected host |
| pnpm | 12.9.1 | npm wrapper + platform-matched `@pnpm/exe.*-arm64` native binary |
| TypeScript | 7.0.2 | npm wrapper + platform-matched TypeScript ARM64 native compiler |
| esbuild | 0.28.2 | npm wrapper + platform-matched esbuild ARM64 native binary |

Versions, URLs and digests come from [`pins.json`](pins.json), which is tracked
and self-contained. **No version is resolved as `latest`, a range, or by
guesswork.** A clean checkout needs no private design document to build.

## Usage

Install (re-runnable; verifies every digest each time):

```sh
bash tools/bootstrap/bootstrap.sh
```

Before building the Rust workspace, prepare its selected native SQLite engine:

```sh
bash tools/bootstrap/build-sqlite.sh
tools/bootstrap/dev-env.sh cargo build --workspace --locked
tools/bootstrap/dev-env.sh cargo run -p avencrew-store-sqlite --example engine_check --locked
```

The recipe admits macOS ARM64 and native GNU/Linux ARM64 profiles. Linux needs
`libclang-dev`, a C compiler and `ar`; macOS needs Xcode Command Line Tools.
CI runs each on its actual host. This does not establish Linux Electron packaging.
Native sources, archives and diagnostics stay under ignored `build/` output.

Use the tools for one command:

```sh
tools/bootstrap/dev-env.sh cargo --version
```

Or activate for the current shell:

```sh
source tools/bootstrap/dev-env.sh
cargo --version && node --version && pnpm --version && tsc --version
```

To leave a sourced activation, restore your own `PATH` and unset the variables
listed in `$AVENCREW_ENV_VARS`. There is no `deactivate` command — this is a
plain shell script, not a shell framework.

If `build/dev-toolchain/` is missing when you activate, run `bootstrap.sh` first.

## Integrity

Every artifact is checked **before** extraction or execution:

- Rust: the dated `channel-rust-1.99.0.toml` manifest is compared to the pinned
  SHA-256 and byte count, then rustup verifies each component's own hashes. This
  is a hash comparison against the primary source, **not** an independently
  verified publisher signature.
- Node: host archive SHA-256 against its explicit pin.
- pnpm / TypeScript / esbuild and each native payload: npm SHA-512 integrity
  against the manifest, plus the package's own `version` field.
- Native payloads must be Mach-O arm64 on macOS or ELF AArch64 on Linux.

`safe_extract.py` refuses absolute member paths, `..` traversal, links that escape
the archive root, and unexpected member types (device nodes, fifos). It accepts
legitimate intra-tree relative links such as Node's `bin/corepack`.

## What this does not do

- No `rustup default` against your normal home. The managed `RUSTUP_HOME` gets its
  own `settings.toml`; `~/.rustup` is never written.
- No global `npm install`, no `sudo`, no Homebrew changes, no shell-profile edits.
- No Electron, React, SQLx, SQLite engine, PostgreSQL, Lima or guest image here —
  those belong to their own tasks.
- Install lifecycle scripts are not a blanket-approved path; artifacts are placed
  from verified tarballs in a normal published layout instead.
