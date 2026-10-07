/**
 * Public entry point for the Avencrew contract package.
 *
 * `wire.ts` and `wire.schema.json` are GENERATED from crates/contracts.
 * `validate.ts` wraps a build-generated schema/semantic validator with JSON
 * ingress and surface checks. Rust remains authoritative.
 */

export * from './validate.ts';
export * from './wire.ts';
