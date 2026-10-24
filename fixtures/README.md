# Synthetic application fixtures

These files contain invented product evidence and deterministic inputs for the
local harness. They are not model qualification benchmarks, customer data,
production seed records, or evidence of successful application execution.

- `ids.json`: fixed UUIDv7 fixture identities and clock; never production IDs.
- `sources/evidence.json`: versioned evidence, scope, audience, lineage, coverage
  and SHA-256 of compact, sorted-key UTF-8 content JSON.
- `projects/scenarios.json`: five cross-role journeys and ordered event inputs.
- `projects/export/`: conflicting documentation/code/configuration, a failing
  baseline reproduction, candidate correction, and a concurrent human edit.
- `models/responses.json`: wire-valid normalized candidates and raw fragment
  inputs. Text is only a candidate, never an acceptance oracle. An empty usage
  list supplies no usage measurement; the explicit fixture rate is zero.
- `models/streams.json`: frozen finite stream inputs for the first pure Rust
  fixture normalizer; includes incomplete/invalid/replayed streams.
- `models/execution.json`: explicit zero-price supervisor launch input with
  synthetic registration/task, audience-scoped source and two finite model
  episodes. The real engine/broker/SQLite consumer returns a checked candidate
  with required business checks and human review still pending.
- `faults/injections.json`: named injection boundaries for later runtime tests.
- `manifest.json`: immutable file inventory, byte lengths and SHA-256 digests.

Run `bash tools/bootstrap/dev-env.sh node tools/dev/check-fixtures.mjs` to check
inventory, references, content digests and normalized wire shape. It uses no
network, inference, database or new dependency. The baseline reproduction is
intentionally failing; it is input evidence, not a failing application test.

Expected business outcomes remain integrator-owned in private design documents.
Consumers must select authorized sources rather than load this entire corpus
into model context. Private and second-tenant evidence are deliberate probes.
Model streams cannot grant permissions, approve decisions, send messages or
satisfy checks. Process, storage and scheduler injections remain declarative
until the owning implementation executes them. This checker verifies fixture
integrity only; it does not establish any of those runtime behaviors.
