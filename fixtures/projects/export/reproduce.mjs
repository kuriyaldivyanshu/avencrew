// Synthetic check command; no network, credentials or filesystem mutation.
import assert from 'node:assert/strict';
import { total } from './baseline/export.mjs';
const rows = Array.from({ length: 50001 }, () => ({ cents: 1 }));
assert.equal(total(rows), 50001, 'export must include the final row');
