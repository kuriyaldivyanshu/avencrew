/**
 * Small consumption example: read a command receipt as a typed value.
 *
 * Demonstrates the three properties the contract must preserve end to end:
 *   - counters stay decimal strings (`accepted_seq`), never JS numbers
 *   - a nullable field is REQUIRED and typed `T | null`, not `T | undefined`
 *   - the tagged union keeps its wire discriminant, so `kind` narrows the shape
 *
 * Run: node --experimental-strip-types examples/consume.ts
 */
import type { CommandReceipt } from '../src/wire.ts';
import { validateCommandReceiptPayload } from '../src/validate.ts';

// An `applied` receipt: disposition_ref is required and non-null in this variant.
const applied: CommandReceipt = {
  kind: 'applied',
  command_id: '019a5500-0000-7000-8000-000000000001',
  authority: 'server',
  run_id: '019a5500-0000-7000-8000-000000000002',
  // Above 2^53. This is exactly why counters are strings: as a JS number this
  // value would silently lose precision.
  accepted_seq: '9007199254740993',
  event_seq: '128',
  row_version: '8',
  disposition_ref: '019a5500-0000-7000-8000-000000000003',
  task_id: '019a5500-0000-7000-8000-000000000004',
  task_revision_id: '019a5500-0000-7000-8000-000000000005',
};

if (applied.kind !== 'applied') {
  throw new Error('discriminant did not narrow');
}
// Counters are strings: this is a string operation, not arithmetic.
console.log('accepted_seq is a string:', typeof applied.accepted_seq === 'string');
console.log('accepted_seq preserved exactly:', applied.accepted_seq === '9007199254740993');
console.log('BigInt sees the same value:', BigInt(applied.accepted_seq) === 9007199254740993n);

// A pending receipt: no accepted sequence exists, and disposition_ref is null.
const pending: CommandReceipt = {
  kind: 'pending',
  command_id: '019a5500-0000-7000-8000-000000000006',
  authority: 'local',
  reason: 'unsent',
};

if (pending.kind === 'pending') {
  // Narrowing proves the wire tag is a real discriminant, not an erased union.
  console.log('pending reason:', pending.reason);
}

const appliedCheck = validateCommandReceiptPayload(applied);
const pendingCheck = validateCommandReceiptPayload(pending);
console.log('applied accepted:', appliedCheck.ok);
console.log('pending accepted:', pendingCheck.ok);
if (!appliedCheck.ok || !pendingCheck.ok) {
  throw new Error('valid receipts were rejected');
}