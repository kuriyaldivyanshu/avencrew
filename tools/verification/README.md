# Scaffold verification

`check.sh` runs existing checks without installing dependencies, changing pins
or creating a second test framework. It rejects unadmitted platforms with exit
2 and stops on the first failing command. Normal command output supplies the
error; no environment dump or credential capture is performed.

After the [managed setup](../bootstrap/README.md), native SQLite build and
`bash tools/bootstrap/dev-env.sh pnpm install --frozen-lockfile`:

```sh
bash tools/verification/check.sh all
```

Individual lanes: `rust`, `desktop`, `contracts`, `fixtures`, `licenses`. Rust includes the
real native SQLite engine diagnostic. Contract checking includes regeneration
comparison and the existing Rust/TS conformance runner. Fixture checking creates
a fresh temporary seed root, prints its safely quoted cleanup command and does
not start a database, model or desktop session. Its offline Rust check requires
the dependencies cached by the preceding Rust lane.

## CI contract

Inputs are the checked-out revision, tracked tool pins, Cargo/pnpm lockfiles,
canonical Rust DTOs and committed synthetic examples. Private `docs/`, design
`schemas/`, archive code, credentials and installed local binaries are not
inputs. Generation must compare against the checked-out artifacts rather than
rewrite them. Output is step command logs, exit status and a job summary; build
outputs stay disposable on the runner. No deployment, publication or artifact
upload occurs.

The workflow uses clean standard `macos-15` and `ubuntu-24.04-arm` ARM64 runners, an immutable
checkout v6 commit, read-only repository permission and no persisted Git
credential. Managed tool archives are checked by the existing bootstrap;
package installation is frozen with lifecycle scripts disabled by `.npmrc`.
There is no compiled-binary cache to mask a clean-build failure. Concurrent
runs of the same branch/PR are cancelled, and execution is bounded to 45 minutes.

A passing matrix establishes the enabled macOS and portable Linux ARM64 lanes.
Linux Electron distribution, Electron launch, journal behavior, process/VM containment and recovery remain
explicitly unverified. PostgreSQL, cloud and model execution remain disabled.
Never interpret these as passing because the scaffold job passed. Missing pins,
dependencies, native files or generated artifacts fail their actual step; no
`continue-on-error`, placeholder success job or substitute platform is used.

Standard hosted runners are free for this public repository. The job is skipped
if the repository becomes private, requiring a reviewed runner/budget decision;
that skip is not scaffolding acceptance. See GitHub's [runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

## Acceptance

Run the local lanes and check shell/workflow syntax. Verify an unknown lane
exits 2 and the existing generated-artifact drift guard exits nonzero without
rewriting artifacts. Then execute the workflow on GitHub and review its actual
step results against the checked-out SHA. Local success and parsed YAML do not
prove a hosted job ran. Acceptance requires successful results for both actual
hosts at the checked-out SHA. Packaging and runtime gates remain separate.

## Formatting and licenses

`pnpm format:check` and `pnpm lint` use exact Biome 2.5.15, with warnings
treated as failures. `pnpm format` formats maintained TypeScript/JavaScript/CSS
and selected configuration files. Generated wire artifacts are excluded and
checked by the existing generator, so formatting does not rewrite canonical
output. Deliberately malformed conformance fixtures permit `any` and non-null
assertions only in that example file.

The `licenses` lane writes ignored CycloneDX 1.6 component inventory, detailed
license hashes and NOTICES under `build/licenses/<rust-host>.*`. It covers the
installed locked all-feature Rust and npm build/development/runtime closure,
not a shipping-product SBOM. Cargo archive bytes must match Cargo.lock. Missing
crate license text is retained from its exact source revision; native npm
wrapper notices must match version and declared license. Source URLs/hashes
are tracked in `notices/sources.json`; the check never fetches replacement text.
SQLite's public-domain notice and the macOS Electron/Chromium notices are
retained in the output.

Known exception: `@electron-internal/extract-zip@1.0.5` declares BSD-2-Clause
but its archive and published source revision omit copyright/license text.
The inventory prints and records a **redistribution blocker**, retaining the
source declaration without inventing attribution. Resolve upstream notice
before distributing the application. An unknown missing license or text fails
the lane; this version-specific exception does not grant release acceptance.
