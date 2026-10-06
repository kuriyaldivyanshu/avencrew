# @avencrew/contracts-ts

Generated TypeScript types and runtime checks for the Avencrew wire protocol.

**Rust is the only source of truth.** Types come from
[`crates/contracts`](../../crates/contracts) via the two reviewed export
examples. Nothing here is hand-written from a design document, and the private
`docs/` and `schemas/` trees are not needed to build or check this package.

## What is generated vs hand-written

| File | Origin | Edit it? |
|---|---|---|
| `src/wire.ts` | generated — 179 types bundled into one file | **No** |
| `src/wire.schema.json` | generated — JSON Schema 2020-12 | **No** |
| `fixtures/wire-examples.json` | mirrored from `crates/contracts/tests/` | **No** |
| `src/wire-validator.mjs` | generated structural + portable semantic validation | **No** |
| `src/wire-validator.d.mts` | small declaration for generated code | Yes |
| `src/validate.ts` | JSON ingress and client-surface wrappers | Yes, with care |
| `examples/*.ts` | consumption + conformance examples | Yes |

The generator uses exact **Ajv 8.20.0 (MIT)** with its Draft 2020-12 class
and the existing **esbuild 0.28.2** pin, both development dependencies. Ajv's
standalone output is bundled into one browser module: there are no runtime
imports, Node dependencies, schema downloads, `eval` or `new Function` calls.
See [NOTICE](NOTICE) for the retained Ajv license. Its archive SHA-512 matches
public npm metadata and the lockfile; publisher signatures were not verified.

The structural schema supplies required fields, exact tags, unknown-field
rejection, nullable fields, text/list/object bounds and numeric limits. During
compilation, the generator attaches the additional Rust rules to named schema
definitions: int64 counters, valid UTC calendar dates, paired environments,
cursor order, authority/placement consistency and committed transfer epochs.
These also run in **nested responses**, not just top-level messages. Neither
validation nor choosing the internal surface authenticates a caller.

Use `parseWireMessage(text, { surface: 'client' })` for untrusted JSON text.
It rejects duplicate/escaped-equivalent keys, invalid Unicode, excessive depth
and frames over 1 MiB before structural validation. Normalized tool arguments
have the additional 256 KiB UTF-8 limit. The validator never coerces values,
inserts defaults or removes fields; errors do not echo payloads.

`validateMessage` operates on already-decoded JSON values and cannot recover
original duplicate keys or number spellings. JavaScript cannot represent all
of Rust's opaque JSON integers, so the text parser deliberately refuses unsafe
numeric integers and integral decimal/exponent spellings (including `-0`)
rather than silently normalize them. This is a documented stricter client
subset: full-size domain counters and money remain strings, preserving their
precision; ordinary nonintegral JSON numbers remain supported.

## Commands

```sh
pnpm --filter @avencrew/contracts-ts typecheck
pnpm --filter @avencrew/contracts-ts conformance
pnpm --filter @avencrew/contracts-ts check:generated
pnpm --filter @avencrew/contracts-ts check        # all three
```

## Regenerating

```sh
node tools/contracts/generate.mjs            # write
node tools/contracts/generate.mjs --check    # fail if stale, never rewrite
```

`--check` is what CI should run. It compares the committed artifacts against
fresh Rust output and exits nonzero on drift, leaving the files untouched.

## Why one generated file

`ts-rs` emits one module per type (179 files). They are all pure `export type`
with type-only imports and no runtime value, so bundling them into a single
`wire.ts` loses nothing and keeps the package small. The bundler sorts by export
name, so the output is byte-deterministic for a given Rust input.

## Preserved wire properties

The generated types are checked by the conformance runner:

- **Counters and money stay strings.** `accepted_seq: string`, never `number`.
  `9007199254740993` survives round-trip; as a JS number it would not.
- **Nullable fields are required.** `disposition_ref: string | null`, never
  `disposition_ref?: string`. A missing key is a different wire message.
- **Tagged unions keep their discriminant.** `"message_kind": "rpc_request"`,
  `"kind": "pending"` — the wire representation, not an erased union.

## Cross-language conformance

`examples/conformance.ts` compares against Rust directly:

1. **105 synthetic examples** from `fixtures/wire-examples.json` — the same file
   `crates/contracts/tests/conformance.rs` uses — must validate identically on the
   internal surface and on the client surface (where internal envelopes and
   `harness.*` operations are refused).
2. **225 Rust verdicts** from `crates/contracts/examples/export_verdicts.rs`, the
   authoritative accept/reject oracle.
3. **Derived invalid inputs**: unknown field, unknown tag, unsupported version,
   duplicate keys (including escaped equivalents), leading-zero and out-of-range
   counters, impossible calendar dates, wrong fractional digits, unpaired
   environment/generation, and client-side bounds.
4. **Scalar-level checks** mirroring the Rust scalar tests.

The runner also submits raw nested/request/type/byte-limit regressions to Rust
and runs a browser bundle in a VM with string code generation disabled and no
Node globals. It checks input immutability and large/deep/Unicode JSON ingress.
Current result: `1058 passed, 0 failed` (105 examples and the 225 baseline Rust
verdicts, plus additional submitted regressions).

## What this package does not do

- It does not authenticate, and it grants no capability. Grants, leases, entity
  ownership, idempotency and enabled features are the supervisor's to check.
- Rust's `decode_client_request` / `decode_internal_message` remain authoritative
  on the coordinator side. Live session/grant/lease/binding checks, sockets and
  recovery are later phases, not established by this contract package.