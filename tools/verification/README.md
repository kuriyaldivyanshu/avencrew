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

Individual lanes: `rust`, `desktop`, `contracts`, `fixtures`. Rust includes the
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

The workflow uses one clean standard `macos-15` ARM64 runner, an immutable
checkout v6 commit, read-only repository permission and no persisted Git
credential. Managed tool archives are checked by the existing bootstrap;
package installation is frozen with lifecycle scripts disabled by `.npmrc`.
There is no compiled-binary cache to mask a clean-build failure. Concurrent
runs of the same branch/PR are cancelled, and execution is bounded to 45 minutes.

A passing scaffold job establishes only the enabled macOS lanes. Linux ARM64,
Electron launch, journal behavior, process/VM containment and recovery remain
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
prove a hosted job ran. P1-06/P1-06-R remain open until hosted evidence exists;
Linux and broader P1-02-R acceptance are independent gates.
