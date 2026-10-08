# Local journal migrations

P2-01 implements two immutable, dependency-closed slices: `0001` is L0
(namespace, actors/devices, blobs, erasure fences, compatibility records);
`0002` is L1 (tasks/revisions, runs/attempts, commands/events, sessions/branches,
manifests/checkpoints, budget periods and queue). They contain 19 STRICT domain
tables. Future L2–L5 columns/tables are absent. Correct released migrations with
new migrations; never edit an applied checksum or advertise a downgrade.

`LocalStore` embeds these SQL files at compile time. Startup takes a nonblocking
OS lock on a permanent `supervisor.lock` file and holds it through connection
close. SQLx's SQLite migration lock is a no-op, so it is not used as an ownership
guarantee. SQLx transactions commit each slice's DDL, global ledger and
`user_version` together. Its `_sqlx_migrations` is a tool-owned, non-STRICT
exception with SHA-384 checksums, not a workspace/domain record. Workspace
export compatibility records are separate and are not automatically fabricated
at boot.

Startup verifies the pinned engine version/source, WAL, FULL, foreign keys and
5000ms busy timeout, actual schema objects against the same embedded migrations
in an in-memory reference database, migration checksums, quick_check and
foreign_key_check. Unknown newer or modified schemas fail closed. Only the
supervisor has an interface to this store; no raw connection/pool or arbitrary
SQL API is public.

Trusted callers select an absolute data directory. New directories/files are
0700/0600. Existing permissive paths, symlinked runtime entries and hardlinked
files are refused; permissions are not silently repaired. SQLite WAL/SHM files
are checked too. This is an OS-account ownership boundary and advisory locking,
not protection against the account owner/root. Renderer/guest exclusion still
requires P2/P4 process and environment integration; no encryption is claimed.

## Upgrade snapshot decision

Before an L0→L1 upgrade, the owned connection creates a private snapshot with
SQLite `VACUUM INTO`, verifies its quick_check/previous version, syncs the file
and backup directory, and only then migrates. This is an explicit refinement of
the design's backup requirement: SQLx 0.9.0 exposes no safe C backup wrapper and
our code forbids unsafe. SQLite documents [VACUUM INTO](https://www.sqlite.org/lang_vacuum.html)
as a supported alternative to the C backup API that includes a consistent
snapshot of live WAL state. No main-file copy or new database library is used.
Interrupted/failed snapshots are not restore evidence; there is no automatic
restore or downgrade command. Restoring after erasure requires the later fence
and compatibility protocol. Backups contain private metadata and need governed
retention/deletion when those consumers are implemented.

## Run a disposable boot check

After managed setup and native SQLite preparation:

```sh
ROOT_PATH=$(mktemp -d "${TMPDIR:-/tmp}/avencrew-journal.XXXXXX")
bash tools/bootstrap/dev-env.sh cargo run -p avencrew-supervisor --locked -- \
  store-check --data-root "$ROOT_PATH"
# After inspection, delete only the printed disposable directory.
```

Run recovery/constraint tests with:

```sh
bash tools/bootstrap/dev-env.sh cargo test -p avencrew-store-sqlite --locked
```

Tests cover fresh/reopen and L0 upgrade, permissions/writer exclusion,
transactional migration failure, SQLite FULL and BUSY, cross-tenant/same-parent
references, state/attempt uniqueness/cancellation, schema/checksum tampering,
branch ancestry cycles, and actual child process death before/after commit.
State triggers enforce allowed transition classes; live authority, active leases
and fresh business-check results require the later service transactions.
No test claims atomic command acceptance, leases, broker authorization, host/VM
containment or harness recovery; those have their own P2/P3/P4 tasks.
