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

The Electron desktop scaffold, managed toolchain, static SQLite build, canonical Rust wire contracts, generated TypeScript validation and fixture development commands are implemented. Durable application storage, harness execution, workers and cloud services remain future work. The local private [architecture draft](docs/ARCHITECTURE_DRAFT_1.md) and database plans describe the intended application; a public checkout builds without these design documents.

Start with the [documentation review order](docs/README.md).

## Team-Workspace archive

The previous Team-Workspace project is retained in `archive/Team-Workspace/` as a learning reference. Its code and documents record earlier decisions, research, and lessons; they are not the implementation foundation for Avencrew.

New Avencrew plans should live in `docs/`, separate from the old project archive.

## Fixture development (macOS ARM64)

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

The desktop does not consume fixture seeds yet. Linux ARM64 remains unverified.
