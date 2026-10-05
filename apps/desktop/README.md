# @avencrew/desktop

Avencrew Electron desktop client — **development scaffold**. It builds, type-checks
and launches, and it does nothing else.

## What this is not

There is no workspace, task, worker, model, source connection, database, broker or
supervisor control here, and the UI says so rather than showing placeholder
activity. Task admission, the coordinator and the harness are separate phases.

## Requirements

Use the project-managed toolchain. From the repository root:

```sh
bash tools/bootstrap/bootstrap.sh          # once
source tools/bootstrap/dev-env.sh          # or prefix each command
```

That gives the pinned Rust, Node, pnpm, TypeScript and esbuild under
`build/dev-toolchain/`. Your global toolchain is not modified.

To leave a sourced activation, restore your own `PATH` and unset the variables
listed in `$AVENCREW_ENV_VARS`. There is no `deactivate` command.

## Install

```sh
pnpm install --frozen-lockfile # from the repository root
bash tools/bootstrap/fetch-electron.sh   # once: verified Electron binary
```

Run that command directly. It refuses to touch `pnpm-lock.yaml` and fails with
`ERR_PNPM_OUTDATED_LOCKFILE` if `package.json` has drifted, so a build cannot
silently diverge from the committed pins.

Do **not** wrap it in a `package.json` script. Two traps combine: pnpm parses a
script named `install:anything` as the `install` command plus an argument, and
`pnpm run` auto-installs unfrozen before the script body. Together, a "frozen"
install would rewrite the lockfile and then pass against what it had just
written. `.npmrc` sets `verify-deps-before-run=false` to close the second trap.

Two things worth knowing:

- **Install scripts are denied** (`pnpm-workspace.yaml` → `allowBuilds`). esbuild
  and TypeScript resolve their platform packages from the dependency tree.
- **Electron's binary is fetched by `tools/bootstrap/fetch-electron.sh`**, which
  checks the archive's SHA-256 against
  `docs/implementation/p0-release-selection-v1.json` *before* unpacking, then
  places it in the layout `electron/index.js` expects. Re-run it after any
  `rm -rf node_modules`.

## Commands

Run from the repository root:

```sh
pnpm typecheck     # strict tsc, two configs (main+preload, renderer)
pnpm build         # esbuild -> dist/main, dist/preload, dist/renderer
pnpm dev           # build, then launch
```

Or scope to this package:

```sh
pnpm --filter @avencrew/desktop typecheck
pnpm --filter @avencrew/desktop build
pnpm --filter @avencrew/desktop dev
```

No API keys, accounts or paid services are needed.

## Privilege boundaries

These are enforced in code and worth preserving:

| Control | Where |
|---|---|
| `nodeIntegration: false`, `contextIsolation: true`, `sandbox: true` | `src/main/main.ts` |
| Fixed local `loadFile` only | `src/main/main.ts` |
| In-page navigation and popups refused | `src/main/main.ts` |
| CSP `default-src 'none'`, `script-src 'self'`, `connect-src 'none'` | `src/renderer/index.html` |
| Preload exposes one frozen value and no IPC channel | `src/preload/preload.ts` |
| Renderer type environment has no Node globals (`types: []`) | `tsconfig.renderer.json` |

Adding an `ipcRenderer` call to the preload, or an inline script to the HTML, would
break these deliberately. A sandboxed renderer is **not** a command sandbox;
command and tool isolation is the harness's job under the guest profile.

## Layout

```
src/main/main.ts          window, lifecycle, navigation refusal
src/preload/preload.ts    the single contextBridge value
src/shared/               frozen build metadata (the only shared type)
src/renderer/             React view, stylesheet, HTML, ambient types
build.mjs                 three esbuild outputs
tsconfig.main.json        main + preload (Node libs, node types)
tsconfig.renderer.json    renderer (DOM only, no node types)
```