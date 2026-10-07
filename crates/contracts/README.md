# Shared contracts

This crate owns protocol 1.0 wire definitions, scalar spelling/precision, strict
JSON decoding, canonical record bytes and bounded big-endian framing. It imports
no harness, database, Electron or provider implementation. The public wire types
cover the documented protocol; their existence does not enable an operation.

Use `decode_client_request` on authenticated desktop-client input. It rejects
internal harness operations and trusted-context/lease envelopes. Use
`decode_internal_message` only behind a protected, authenticated internal peer.
Neither decoder authenticates a session or validates current grants, leases,
entity ownership, idempotency or enabled capabilities. Those checks remain with
the supervisor/broker. Raw Serde decoding bypasses duplicate-key, frame-limit,
client-surface and cross-field checks; it is not an application ingress API.
`encode_message` validates constructed messages; `framing::encode_frame` adds
the length prefix. `FrameDecoder::feed` consumes one frame at a time, returns
its consumed byte count and never exposes a partial message. After an invalid
frame discard the decoder/connection. At EOF call `finish`.

Counters remain decimal strings through serialization and TS export, including
values above 2^53. `Counter` permits zero; active lease/checkpoint epochs and
revisions use `PositiveCounter`. Historical events can carry epoch zero before
the first attempt. Deletion generation starts at zero. Nullable fields must be
present with either a value or null. Instants require valid UTC calendar time
with exactly six fractional digits. Text bounds count Unicode scalar values;
the overall frame bound counts UTF-8 bytes.

`canonical_bytes` rejects duplicate keys and floating numbers, sorts keys by
Unicode scalar order, preserves Unicode normalization form and emits compact
UTF-8 JSON. Hash the returned bytes in the owning storage/broker consumer; this
crate does not grant validity or audience permission to a canonical bundle.
Model proposal arguments are capped at 256 KiB of serialized JSON. Tool arguments
and binding-specific result data otherwise remain opaque here. The broker
must validate their sealed tool schema and lowered byte limits before dispatch.
Physical bundle records/FK closure, migrations, live authorization, execution,
sockets and business completion are implemented by their owning later tasks.

Run from the repository root using the managed toolchain:

```sh
bash tools/bootstrap/dev-env.sh cargo test -p avencrew-contracts --all-features --locked
bash tools/bootstrap/dev-env.sh cargo run -p avencrew-contracts --example export_schema --features schema-export --locked
bash tools/bootstrap/dev-env.sh cargo run -p avencrew-contracts --example export_typescript --features typescript-export --locked -- build/contracts-export/ts
```

Rust is the canonical source. P1-04 creates the TS package and deterministic
regeneration commands from the `schema-export` and `typescript-export` features;
never edit generated TS/schema independently or load private design files at
application runtime. JSON Schema covers structure/bounds; TS validators must
also apply the documented counter/calendar/cross-field rules and reject
repeated keys before ordinary JSON parsing. TS types alone cannot enforce that.
The application conformance examples under `tests/` use synthetic data and are
not model qualification exercises.

The selected ts-rs version reports that enum `deny_unknown_fields` is unsupported
for TS generation. Keep the separate Serde tag attributes: an unrecognized
combined attribute would also drop the tag. Rust and schema rejection checks
remain enabled; TS runtime rejection is a P1-04 responsibility. Do not suppress
these diagnostics by loosening Rust unknown-field checks.
