# Local journal migrations

The journal uses immutable, dependency-closed slices: `0001` is L0
(namespace, actors/devices, blobs, erasure fences, compatibility records);
`0002` is L1 (tasks/revisions, runs/attempts, commands/events, sessions/branches,
manifests/checkpoints, budget periods and queue). Those two slices contain 19
STRICT domain tables. `0003` adds the L2 immutable prepared-turn seed and lease
closure. `0004` adds the L2 fixture step/model/invocation/dispatch, artifact/version
and usage records, plus a versioned runtime-progress pointer. Current schema 4
contains 27 STRICT domain tables. Later process/wait/environment/cloud slices
remain absent. Correct released migrations with
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

Tests cover fresh/reopen and upgrades, permissions/writer exclusion,
transactional migration failure, SQLite FULL and BUSY, cross-tenant/same-parent
references, state/attempt uniqueness/cancellation, schema/checksum tampering,
branch ancestry cycles, and actual child process death before/after commit.
State triggers enforce allowed transition classes. The explicit supervisor
fixture profile now composes live identity/control/lease checks, bounded engine
execution, atomic model/proposal/dispatch/result journals, exact fixture-ledger
checkpoints and version-bound candidate checks. Process kills around actual SQL
commits establish crash-cut behavior for that profile; they do not establish
power-loss behavior, VM containment, live filesystem capture, restored-backup
activation or business completion.

Fixture restart also verifies historical applied/superseded control dispositions
against the original receipts and current journal cursors. Accepted stop cannot
be cleared by restart or duplicate requests. Unresolved model intents and tool
dispatches remain in cancellation outcomes; their attempt stays `draining`
without an end time until a future reconciliation owner can prove safe release.
No additional migration or automatic execution/cleanup grant is involved.


## Fixture restart contract

The enabled execution consumer supports one ordinal-one local fixture turn per
run. Startup recovery and explicit restore verify canonical accepted-command
bytes and original receipts, acceptance order, typed applied/superseded
dispositions, progress versions, attempt/epoch, observation references, applied
cursor and at most one immutable outcome. Unknown or future multi-turn histories
remain held; they never grant dispatch. Malformed supported history is refused.

A recovery page admits at most 256 runs, with at most 1024 commands and 2048
events per run. Original command payload recovery has an 8 MiB page budget. The
additional execution verifier has a separate 8 MiB page budget for command and
progress ledger bytes, reserved from registered sizes before opening that run's
history. A budget-held run has `history_complete = false` and no recovered
controls; other eligible runs can still be inspected. These are ledger replay
budgets, not an aggregate bound on identity, checkpoint or artifact validation IO.

Explicit restore reuses verified command bytes, snapshot and context within one
owned-store operation. No verification cache or authority survives the call;
control publication still rechecks live identity, epoch and cursors inside its
transaction. Cancellation retains the exact combined unresolved tool/model set,
with the same maximum of 100 references as the cancellation wire contract.
Unknown work retains its draining reservation until reconciliation proves release.

New stop, pause or steering commands after a committed fixture outcome are
rejected with a conflict rather than receiving an unappliable accepted receipt.
Exact retries of previously accepted commands return their original receipt;
terminal runs retain their terminal no-op receipt. Legacy already-accepted late
controls remain held and cannot replay an old outcome as current. Human artifact
edits retain independent stop, current-version, identity and lease fences; a
rejected late command never claims to have cancelled those edits. A later run
lifecycle consumer must own controls during post-outcome verification/rework.

Regression evidence lives in `commands/execution/tests/control_recovery.rs`,
`commands/execution/tests.rs` and the supervisor's `execution/tests/` modules.
