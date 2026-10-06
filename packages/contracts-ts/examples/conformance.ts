/**
 * Cross-language conformance: the TypeScript checks must agree with Rust.
 *
 * Two sources of truth are combined:
 *   1. `crates/contracts/tests/wire-examples.json` — the SAME synthetic examples
 *      the Rust `conformance` test uses. Copied into `fixtures/` by the runner so
 *      the package never reads gitignored docs or paths outside itself.
 *   2. `crates/contracts/examples/export_verdicts.rs` — Rust's authoritative
 *      accept/reject verdict per example and per derived-invalid input.
 *
 * For each example we compare `accept`, and we also assert that our checks fire
 * for the SAME documented reason where one is recorded. This is agreement
 * testing, not re-implementation: Rust is authoritative.
 *
 * Run: node --experimental-strip-types examples/conformance.ts
 */

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { buildSync } from 'esbuild';

import {
  parseStrictJson,
  parseWireMessage,
  validateCounter,
  validateInstant,
  validateMessage,
  validateMoney,
} from '../src/validate.ts';

const HERE = dirname(fileURLToPath(import.meta.url));
const PKG = resolve(HERE, '..');
const ROOT = resolve(PKG, '..', '..');
const DEV_ENV = join(ROOT, 'tools', 'bootstrap', 'dev-env.sh');
const EXAMPLES = join(PKG, 'fixtures', 'wire-examples.json');

type Verdict = { case: string; accept: boolean; error?: string };
type WireExample = { name: string; message: Record<string, unknown> };

function cargoExample(name: string, args: string[]): string {
  return execFileSync(
    DEV_ENV,
    ['cargo', 'run', '-q', '-p', 'avencrew-contracts', '--example', name, ...args],
    {
      cwd: ROOT,
      encoding: 'utf8',
      maxBuffer: 256 * 1024 * 1024,
    },
  );
}

let passed = 0;
let failed = 0;
function check(name: string, condition: boolean, detail = ''): void {
  if (condition) {
    passed += 1;
  } else {
    failed += 1;
    console.error(`  FAIL ${name}${detail ? ` :: ${detail}` : ''}`);
  }
}

const examples: WireExample[] = JSON.parse(readFileSync(EXAMPLES, 'utf8'));
const verdicts: Verdict[] = JSON.parse(cargoExample('export_verdicts', ['--locked']));

// ---- 1. every documented example: internal accept ----------------------------
const validInternal = verdicts.filter((v) => v.case.startsWith('valid-internal/'));
check(
  'verdict oracle produced internal verdicts',
  validInternal.length === examples.length,
  `${validInternal.length} vs ${examples.length} examples`,
);
for (const example of examples) {
  const verdict = validInternal.find((v) => v.case === `valid-internal/${example.name}`);
  const mine = validateMessage(example.message, { surface: 'internal' });
  check(
    `valid-internal/${example.name}`,
    verdict?.accept === mine.ok,
    `rust=${verdict?.accept} ts=${mine.ok}${mine.ok ? '' : ' ' + JSON.stringify(mine.violations)}`,
  );
}

// ---- 2. every documented example: client surface ------------------------------
const clientVerdicts = verdicts.filter((v) => v.case.startsWith('client-surface/'));
for (const example of examples) {
  const verdict = clientVerdicts.find((v) => v.case === `client-surface/${example.name}`);
  const mine = validateMessage(example.message, { surface: 'client' });
  check(
    `client-surface/${example.name}`,
    verdict?.accept === mine.ok,
    `rust=${verdict?.accept} ts=${mine.ok}${mine.ok ? '' : ' ' + JSON.stringify(mine.violations)}`,
  );
}

// ---- 3. derived invalid inputs: TS must also reject ---------------------------
// These mirror the mutations the Rust oracle applies.
const lease = examples.find((e) => e.message.message_kind === 'lease')!.message;
const receipt = examples.find((e) => e.message.message_kind === 'command_receipt')!.message;

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

const leaseBase = clone(lease) as Record<string, any>;
const invalidCases: Array<[string, () => unknown]> = [
  [
    'leading-zero-counter',
    () => {
      const v = clone(leaseBase);
      v.payload.epoch = '01';
      return v;
    },
  ],
  [
    'counter-out-of-int64',
    () => {
      const v = clone(leaseBase);
      v.payload.epoch = '9223372036854775808';
      return v;
    },
  ],
  [
    'calendar-date',
    () => {
      const v = clone(leaseBase);
      v.payload.expires_at = '2026-02-30T00:00:00.000000Z';
      return v;
    },
  ],
  [
    'instant-fraction',
    () => {
      const v = clone(leaseBase);
      v.payload.expires_at = '2026-10-06T00:00:00.000Z';
      return v;
    },
  ],
  [
    'short-digest',
    () => {
      const v = clone(leaseBase);
      v.payload.receipt_digest = 'AAAA';
      return v;
    },
  ],
  [
    'unknown-field',
    () => {
      const v = clone(leaseBase);
      v.payload.unexpected = true;
      return v;
    },
  ],
  [
    'unpaired-generation',
    () => {
      const v = clone(leaseBase);
      v.payload.generation = '1';
      return v;
    },
  ],
  [
    'local-authority-not-cloud',
    () => {
      const v = clone(leaseBase);
      v.payload.authority = 'server';
      v.payload.placement = 'cloud';
      v.payload.environment_id = null;
      return v;
    },
  ],
  [
    'unknown-receipt-tag',
    () => {
      const v = clone(receipt) as Record<string, any>;
      v.payload.kind = 'nonsense';
      return v;
    },
  ],
  [
    'unsupported-version',
    () => {
      const v = clone(leaseBase);
      v.schema_version = '2.0';
      return v;
    },
  ],
  [
    'counter-as-number',
    () => {
      const v = clone(leaseBase);
      v.payload.epoch = 1;
      return v;
    },
  ],
];

for (const [name, build] of invalidCases) {
  const expectedAccept = name === 'local-authority-not-cloud';
  const rustCase = `${expectedAccept ? 'valid' : 'invalid'}/${name}`;
  const rustVerdict = verdicts.find((v) => v.case === rustCase);
  const mine = validateMessage(build(), { surface: 'internal' });
  check(
    `invalid/${name}`,
    mine.ok === expectedAccept,
    `expected accept=${expectedAccept} got ts=${mine.ok}${mine.ok ? '' : ' ' + JSON.stringify(mine.violations)}`,
  );
  check(
    `${rustCase} agrees with Rust`,
    rustVerdict != null && rustVerdict.accept === expectedAccept,
    rustVerdict ? `rust=${rustVerdict.accept}` : 'no Rust verdict',
  );
}

// ---- 4. duplicate keys (raw text; JSON.parse would silently collapse) --------
const duplicateKeys: Array<[string, string]> = [
  ['duplicate-top-level', '{"schema_version":"1.0","schema_version":"1.0"}'],
  [
    'duplicate-nested',
    '{"schema_version":"1.0","message_kind":"lease","payload":{"run_id":"a","run_id":"b"}}',
  ],
  ['duplicate-escaped-equivalent', '{"schema_version":"1.0","schema_\\u0076ersion":"1.0"}'],
];
for (const [name, text] of duplicateKeys) {
  const parsed = parseStrictJson(text);
  check(`duplicate-key/${name}`, !parsed.ok, 'duplicate key was not rejected');
}
// A text with no duplicates must still parse.
check('strict-parse/clean', parseStrictJson('{"a":1,"b":{"c":2}}').ok);

// ---- 5. scalar-level checks matching the Rust conformance test ---------------
for (const good of ['0', '9007199254740993', '9223372036854775807']) {
  check(`counter/valid/${good}`, validateCounter(good).ok);
}
for (const bad of ['01', '+1', '-1', '1.0', '1e2', '9223372036854775808', '']) {
  check(`counter/invalid/${bad || '<empty>'}`, !validateCounter(bad).ok);
}
check('counter/above-2^53 preserved', validateCounter('9007199254740993').ok);
check('instant/valid-micros', validateInstant('1970-01-01T00:00:00.000001Z').ok);
for (const bad of [
  '2026-02-30T00:00:00.000000Z',
  '2026-10-06T00:00:00.000Z',
  '2026-10-06T00:00:00.000000+00:00',
  '2026-10-06T23:59:60.000000Z',
]) {
  check(`instant/invalid/${bad}`, !validateInstant(bad).ok);
}
check('money/valid', validateMoney('12.000001').ok);
check('money/invalid-precision', !validateMoney('12.00001').ok);
check('money/invalid-final-newline', !validateMoney('12.000001\n').ok);
check('counter/invalid-final-newline', !validateCounter('1\n').ok);
check('instant/invalid-final-newline', !validateInstant('1970-01-01T00:00:00.000001Z\n').ok);

// ---- 6. generated invariants (task 6) ---------------------------------------
const wireSource = readFileSync(join(PKG, 'src', 'wire.ts'), 'utf8');
// Counters and money are strings in the generated types, never numbers.
check(
  'generated/counters-are-strings',
  /accepted_seq: string/.test(wireSource) && !/accepted_seq: number/.test(wireSource),
);
check(
  'generated/nullable-is-required-string-union',
  /disposition_ref: string \| null/.test(wireSource),
);
check('generated/nullable-not-optional', !/disposition_ref\?:/.test(wireSource));
// Tagged unions retain their wire representation (discriminant present, not erased).
check('generated/tagged-union-receipt', /"kind": "pending"/.test(wireSource));
check('generated/tagged-union-message', /"message_kind": "rpc_request"/.test(wireSource));

// Regression cases submitted as raw JSON to the same Rust ingress functions.
const regressions: Array<{ case: string; text: string; client: boolean }> = [];
function mutation(
  name: string,
  source: Record<string, unknown>,
  change: (v: any) => void,
  client = false,
): void {
  const value = clone(source);
  change(value);
  regressions.push({ case: name, text: JSON.stringify(value), client });
}
const request = examples.find(
  (e) =>
    (e.message.payload as any).operation === 'handshake' &&
    e.message.message_kind === 'rpc_request',
)!.message;
mutation(
  'request/bad-id',
  request,
  (v) => {
    v.payload.request_id = 'not-a-uuid';
  },
  true,
);
mutation(
  'request/id-final-newline',
  request,
  (v) => {
    v.payload.request_id += '\n';
  },
  true,
);
mutation('lease/counter-final-newline', lease, (v) => {
  v.payload.epoch += '\n';
});
mutation('lease/instant-final-newline', lease, (v) => {
  v.payload.expires_at += '\n';
});
mutation('lease/digest-final-newline', lease, (v) => {
  v.payload.receipt_digest += '\n';
});
mutation(
  'request/empty-body',
  request,
  (v) => {
    v.payload.body = {};
  },
  true,
);
mutation(
  'request/body-extra-field',
  request,
  (v) => {
    v.payload.body.unauthorized = true;
  },
  true,
);
mutation('request/unknown-operation', request, (v) => {
  v.payload.operation = 'imaginary';
});
for (const example of examples) {
  mutation(`payload/extra/${example.name}`, example.message, (v) => {
    v.payload.unexpected = true;
  });
}
const taskRequest = examples.find(
  (e) =>
    (e.message.payload as any).operation === 'submit_task' &&
    e.message.message_kind === 'rpc_request',
)!.message;
mutation(
  'request/missing-required-null',
  taskRequest,
  (v) => {
    delete v.payload.body.project_id;
  },
  true,
);
mutation(
  'request/objective-limit',
  taskRequest,
  (v) => {
    v.payload.body.objective = 'x'.repeat(8193);
  },
  true,
);
mutation(
  'request/source-list-limit',
  taskRequest,
  (v) => {
    v.payload.body.source_refs = Array(101).fill(v.payload.request_id);
  },
  true,
);
mutation(
  'request/integer-type',
  taskRequest,
  (v) => {
    v.payload.body.lower_limits.model_steps = '1';
  },
  true,
);
mutation(
  'request/integer-limit',
  taskRequest,
  (v) => {
    v.payload.body.lower_limits.model_steps = 51;
  },
  true,
);
mutation('lease/local-cloud', lease, (v) => {
  v.payload.authority = 'local';
  v.payload.placement = 'cloud';
});
const snapshot = examples.find(
  (e) =>
    e.message.message_kind === 'rpc_response' &&
    (e.message.payload as any).operation === 'get_task_snapshot' &&
    (e.message.payload as any).ok,
)!.message;
mutation('nested/cursor-order', snapshot, (v) => {
  v.payload.result.accepted_seq = '0';
  v.payload.result.applied_seq = '1';
});
mutation('nested/counter-overflow', snapshot, (v) => {
  v.payload.result.last_event_seq = '9223372036854775808';
});
mutation('nested/local-cloud', snapshot, (v) => {
  v.payload.result.authority = 'local';
  v.payload.result.placement = 'cloud';
});
const transfer = examples.find(
  (e) =>
    e.message.message_kind === 'rpc_response' &&
    ['prepare_transfer', 'query_transfer'].includes((e.message.payload as any).operation) &&
    (e.message.payload as any).ok,
)!.message;
for (const [name, change] of [
  [
    'epoch',
    (v: any) => {
      v.payload.result.destination_epoch = v.payload.result.source_epoch;
    },
  ],
  [
    'authority',
    (v: any) => {
      v.payload.result.authority = 'local';
    },
  ],
  [
    'receipt',
    (v: any) => {
      v.payload.result.receipt_ref = null;
    },
  ],
] as const) {
  mutation(`nested/committed-transfer-${name}`, transfer, (v) => {
    v.payload.result.disposition = 'committed';
    change(v);
  });
}
const model = clone(
  examples.find((e) => e.message.message_kind === 'normalized_model_response')!.message,
);
(model.payload as any).items = [
  {
    kind: 'tool_proposal',
    provider_call_id: 'test',
    binding_id: (request.payload as any).request_id,
    arguments: {},
  },
];
check('model/tool-proposal-baseline-valid', validateMessage(model).ok);
mutation('arguments/utf8-byte-limit', model, (v) => {
  v.payload.items.find((item: any) => item.kind === 'tool_proposal').arguments = {
    text: '🌍'.repeat(65536),
  };
});
regressions.push({
  case: 'raw/duplicate-key',
  text: JSON.stringify(request).replace(
    '"schema_version":"1.0"',
    '"schema_version":"1.0","schema_\\u0076ersion":"1.0"',
  ),
  client: true,
});
regressions.push({
  case: 'raw/frame-byte-limit',
  text: ' '.repeat(1048577) + JSON.stringify(request),
  client: true,
});
regressions.push({
  case: 'raw/lone-surrogate',
  text: JSON.stringify(taskRequest).replace('"objective":"x"', '"objective":"\\ud800"'),
  client: true,
});
const regressionVerdicts: Verdict[] = JSON.parse(
  execFileSync(
    DEV_ENV,
    [
      'cargo',
      'run',
      '-q',
      '-p',
      'avencrew-contracts',
      '--example',
      'export_verdicts',
      '--locked',
      '--offline',
      '--',
      '--stdin',
    ],
    { cwd: ROOT, encoding: 'utf8', input: JSON.stringify(regressions) },
  ),
);
for (const [index, test] of regressions.entries()) {
  const verdict = regressionVerdicts[index];
  const mine = parseWireMessage(test.text, { surface: test.client ? 'client' : 'internal' });
  check(`regression/${test.case}/rust-rejects`, verdict?.accept === false);
  check(`regression/${test.case}/ts-agrees`, mine.ok === verdict?.accept);
}
check('strict-parse/sentinel-string-is-valid-json', parseStrictJson('"\\u0000duplicate-key"').ok);
check('strict-parse/astral-unicode', parseStrictJson('"🌍"').ok);
check('strict-parse/lone-surrogate', !parseStrictJson('"\\ud800"').ok);
check('strict-parse/unsafe-integer', !parseStrictJson('{"n":9007199254740993}').ok);
check('strict-parse/integral-float-spelling', !parseStrictJson('{"n":1e0}').ok);
check('strict-parse/fraction', parseStrictJson('{"n":0.5}').ok);
check(
  'strict-parse/array-with-many-items',
  parseStrictJson(JSON.stringify(Array(150000).fill(0))).ok,
);
check('strict-parse/depth', !parseStrictJson('['.repeat(129) + '0' + ']'.repeat(129)).ok);

for (const example of examples) {
  for (const badPayload of [null, [], false, 1, 'invalid']) {
    const value = clone(example.message);
    value.payload = badPayload;
    check(`payload/type/${example.name}/${JSON.stringify(badPayload)}`, !validateMessage(value).ok);
  }
}
const beforeValidation = JSON.stringify(request);
validateMessage(request);
check('schema/input-not-mutated', JSON.stringify(request) === beforeValidation);

// Prove the whole runtime wrapper can execute without Node or dynamic code.
const browser = buildSync({
  entryPoints: [join(PKG, 'src', 'validate.ts')],
  bundle: true,
  platform: 'browser',
  format: 'iife',
  globalName: 'Contracts',
  target: 'es2022',
  write: false,
}).outputFiles[0]!.text;
const context = { TextEncoder, input: request };
check(
  'csp/no-dynamic-evaluation',
  runInNewContext(browser + '; Contracts.validateMessage(input, {surface:"client"}).ok', context, {
    contextCodeGeneration: { strings: false, wasm: false },
    timeout: 5000,
  }) === true,
);
check(
  'csp/no-node-globals',
  runInNewContext('typeof process === "undefined" && typeof require === "undefined"', context),
);
check(
  'generated/runtime-has-no-imports',
  !/^import /m.test(readFileSync(join(PKG, 'src', 'wire-validator.mjs'), 'utf8')),
);

// ---- summary ----------------------------------------------------------------
console.log(
  `conformance: ${passed} passed, ${failed} failed (${examples.length} examples, ${verdicts.length} Rust verdicts)`,
);
if (failed > 0) process.exit(1);
