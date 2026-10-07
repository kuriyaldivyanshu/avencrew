# Avencrew

A shared workspace for people and AI workers across sales, product management, and engineering.

## Product direction

Avencrew will bring team context, work, and deliverables into one workspace. AI workers will support email, reports, spreadsheets, coding, scheduled work, and long-running tasks.

Sales, product management, and engineering are in scope from the first release. They share projects, tasks, artifacts, workers, and capabilities. Work can begin with any teammate and span several functions; department labels do not partition the product or determine capability access.

Planned capabilities include:

- An owned agent runtime and harness, with model APIs behind it.
- Recoverable task execution, schedules, progress tracking, and human review.
- Cloud workers that continue when the app closes, with scoped sandbox execution and persistent task context/files; local development validates the same contracts.
- Plugins, connected tools, and skill creation and import.
- Worker building and shared artifacts across sales, product, and engineering.

## Current status

The Electron desktop scaffold, managed toolchain, static SQLite build, canonical Rust wire contracts, generated TypeScript validation and fixture development commands are implemented. The P2 local journal now boots 19 STRICT tables with exclusive supervisor ownership and transactional migrations. Standalone human registration and private first-version task/draft-artifact canonical publication are implemented in the store, with digest/closure checks and atomic references. Trusted local run registration and steer/pause/stop acceptance now persist command receipts, events and reconciliation intents atomically. The supervisor now owns a restricted Unix socket with OS peer/session authentication, bounded wire frames and durable command receipts; client disconnection does not shut it down. Ordered control application/recovery, Electron socket integration, executable model admission, harness execution, workers and cloud services remain future work. The local private [architecture draft](docs/ARCHITECTURE_DRAFT_1.md) and database plans describe the intended application; a public checkout builds without these design documents.

Start with the [documentation review order](docs/README.md).

## Team-Workspace archive

The previous Team-Workspace project is retained in `archive/Team-Workspace/` as a learning reference. Its code and documents record earlier decisions, research, and lessons; they are not the implementation foundation for Avencrew.

New Avencrew plans should live in `docs/`, separate from the old project archive.

## Fixture development (macOS ARM64 and GNU/Linux ARM64)

Use the [managed toolchain setup](tools/bootstrap/README.md) first. Install
workspace dependencies and cache the contract test dependencies once:

```sh
bash tools/bootstrap/dev-env.sh pnpm install --frozen-lockfile
bash tools/bootstrap/dev-env.sh cargo test -p avencrew-contracts --all-features --locked --tests
```

Then run the fixture lane, including offline Rust conformance checks:

```sh
bash tools/bootstrap/dev-env.sh pnpm dev:fixture
```

This copies synthetic contract examples into a fresh temporary directory and
prints its location. It does not start Electron or create a database. After
setup, no network or model credentials are needed. If the offline check reports
missing cached dependencies, repeat the setup commands while connected.

An explicit `--data-root DIR` must resolve to a temporary subdirectory. Existing
nonempty roots are reused only when their four seed files match exactly;
unrelated files and symlinks are refused. There is no non-temporary override.
To open the current desktop scaffold separately:

```sh
bash tools/bootstrap/fetch-electron.sh
bash tools/bootstrap/dev-env.sh pnpm dev
```

The desktop does not consume fixture seeds yet. Electron distribution and launch
are macOS-only; the Linux CI lane builds portable Rust/contracts and bundles.

## Continuous integration

[Scaffold checks](.github/workflows/scaffold.yml) run on pushes to `main` and
`setup/initial-foundation`, PRs targeting `main`, and manual dispatch. The
workflow uses standard macOS and Linux ARM64 runners and the tracked toolchain/native
recipes. Rust, desktop, contracts, fixtures and dependency inventory have separate steps.

Run the same checks locally after dependency setup:

```sh
bash tools/verification/check.sh all
```

See [verification scope and prerequisites](tools/verification/README.md).
Linux Electron distribution, desktop launch and runtime recovery/isolation are
not established by a passing scaffold matrix. This public-repository workflow skips private repos
rather than automatically switching to billed execution.

Formatting: `bash tools/bootstrap/dev-env.sh pnpm format`; checking:
`bash tools/bootstrap/dev-env.sh pnpm format:check` and
`bash tools/bootstrap/dev-env.sh pnpm lint`. Generated contracts stay
generator-owned. Dependency inventories and known redistribution limitations
are described in the verification guide.

## Local journal boot

The supervisor now supports `store-check --data-root ABSOLUTE_DIRECTORY`,
which migrates/validates the journal and closes it. Use a disposable directory
while developing; see [local migrations and recovery checks](migrations/sqlite/README.md).
It does not execute tasks or start a harness.
