#!/usr/bin/env node
// Deterministic contract artifact generation.
//
//   node tools/contracts/generate.mjs           write artifacts
//   node tools/contracts/generate.mjs --check   fail if artifacts are stale (never writes)
//
// Canonical source is Rust (crates/contracts). This script only orchestrates the
// two existing export examples and bundles their per-type output into ONE
// TypeScript file, because the task asks for a single generated file and the
// 179 per-type modules have no runtime value.
//
// It does not read, need, or embed any private design schema under docs/ or
// schemas/: a clean checkout regenerates from Rust alone.
//
// Determinism: ts-rs emits one type per file; we sort by export name, drop the
// header and the type-only imports (all symbols live in the same module after
// bundling), and emit a fixed preamble. Same Rust input -> byte-identical output.

import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..', '..');
const DEV_ENV = join(ROOT, 'tools', 'bootstrap', 'dev-env.sh');
const OUT_DIR = join(ROOT, 'packages', 'contracts-ts');
const SRC_DIR = join(OUT_DIR, 'src');
const WIRE_TS = join(SRC_DIR, 'wire.ts');
const SCHEMA_JSON = join(SRC_DIR, 'wire.schema.json');
const FIXTURES_DIR = join(OUT_DIR, 'fixtures');
const WIRE_EXAMPLES_SRC = join(ROOT, 'crates', 'contracts', 'tests');
const WIRE_EXAMPLES = join(WIRE_EXAMPLES_SRC, 'wire-examples.json');
const WIRE_EXAMPLES_COPY = join(FIXTURES_DIR, 'wire-examples.json');

const CHECK_ONLY = process.argv.includes('--check');

// Build-only dependencies belong to the client contract package. No compiler or
// Node module is imported by the generated runtime validator.
const require = createRequire(join(OUT_DIR, 'package.json'));
const Ajv2020 = require('ajv/dist/2020').default;
const standaloneCode = require('ajv/dist/standalone').default;
const { _, _Code } = require('ajv/dist/compile/codegen/code');
const { buildSync } = require('esbuild');

// Supplement the exported structural schema with the portable Rust rules.
// These small functions are embedded into generated code, including at nested
// occurrences of the named definitions. Keep aligned with scalars/semantics.rs.
const portableRules = {
  counter: function counter(s) {
    return typeof s === 'string' && /^(0|[1-9][0-9]{0,18})$/.test(s)
      && BigInt(s) <= 9223372036854775807n;
  },
  instant: function instant(s) {
    if (typeof s !== 'string' || !/^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{6}Z$/.test(s)) return false;
    const y = Number(s.slice(0, 4)), m = Number(s.slice(5, 7)), d = Number(s.slice(8, 10));
    const leap = y % 4 === 0 && (y % 100 !== 0 || y % 400 === 0);
    const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    return m >= 1 && m <= 12 && d >= 1 && d <= days[m - 1]
      && Number(s.slice(11, 13)) < 24 && Number(s.slice(14, 16)) < 60
      && Number(s.slice(17, 19)) < 60;
  },
  lease: function lease(v) {
    if (v === null || typeof v !== 'object' || Array.isArray(v)) return false;
    return (v.environment_id === null) === (v.generation === null)
      && !(v.authority === 'local' && v.placement === 'cloud');
  },
  context: function context(v) {
    if (v === null || typeof v !== 'object' || Array.isArray(v)) return false;
    return (v.environment_id === null) === (v.generation === null);
  },
  checkpoint: function checkpoint(v) {
    if (v === null || typeof v !== 'object' || Array.isArray(v)) return false;
    return typeof v.applied_seq === 'string' && typeof v.accepted_seq === 'string'
      && /^(0|[1-9][0-9]{0,18})$/.test(v.applied_seq)
      && /^(0|[1-9][0-9]{0,18})$/.test(v.accepted_seq)
      && BigInt(v.applied_seq) <= BigInt(v.accepted_seq);
  },
  snapshot: function snapshot(v) {
    if (v === null || typeof v !== 'object' || Array.isArray(v)) return false;
    return !(v.authority === 'local' && v.placement === 'cloud')
      && typeof v.applied_seq === 'string' && typeof v.accepted_seq === 'string'
      && /^(0|[1-9][0-9]{0,18})$/.test(v.applied_seq)
      && /^(0|[1-9][0-9]{0,18})$/.test(v.accepted_seq)
      && BigInt(v.applied_seq) <= BigInt(v.accepted_seq);
  },
  transfer: function transfer(v) {
    if (v === null || typeof v !== 'object' || Array.isArray(v)) return false;
    if (v.disposition !== 'committed') return true;
    return v.authority === 'server' && v.receipt_ref != null
      && typeof v.source_epoch === 'string' && typeof v.destination_epoch === 'string'
      && /^[1-9][0-9]{0,18}$/.test(v.source_epoch)
      && /^[1-9][0-9]{0,18}$/.test(v.destination_epoch)
      && BigInt(v.destination_epoch) === BigInt(v.source_epoch) + 1n;
  },
};

function validatorSource(schemaJson) {
  const schema = JSON.parse(schemaJson);
  // Tagged oneOf branches can dispatch directly instead of probing every DTO.
  // Apply only when every branch requires a distinct literal string tag.
  function tagUnions(node) {
    if (!node || typeof node !== 'object') return;
    // JS `$` also matches before a final newline; Rust's scalar constructors
    // require the complete canonical spelling. Preserve that stricter boundary.
    if (typeof node.pattern === 'string' && node.pattern.endsWith('$')) {
      node.pattern += '(?![\\s\\S])';
    }
    if (Array.isArray(node.oneOf)) {
      for (const name of ['message_kind', 'operation', 'kind', 'status']) {
        const tags = node.oneOf.map((branch) => branch.properties?.[name]?.const);
        if (tags.every((tag) => typeof tag === 'string')
          && new Set(tags).size === tags.length
          && node.oneOf.every((branch) => branch.required?.includes(name))) {
          node.discriminator = { propertyName: name };
          node.type = 'object';
          break;
        }
      }
    }
    for (const value of Object.values(node)) tagUnions(value);
  }
  tagUnions(schema);
  const annotations = {
    Counter: 'counter', PositiveCounter: 'counter', Instant: 'instant',
    Lease: 'lease', TrustedContext: 'context', Checkpoint: 'checkpoint',
    TaskSnapshot: 'snapshot', TransferReceipt: 'transfer',
  };
  for (const [name, rule] of Object.entries(annotations)) {
    if (!schema.$defs[name]) throw new Error(`missing portable-rule definition: ${name}`);
    schema.$defs[name].avencrewPortable = rule;
  }
  const ajv = new Ajv2020({
    strict: true, allErrors: false, inlineRefs: false, ownProperties: true,
    discriminator: true,
    coerceTypes: false, useDefaults: false, removeAdditional: false,
    code: { source: true, esm: true, optimize: true },
  });
  ajv.addKeyword({
    keyword: 'avencrewPortable', schemaType: 'string',
    code(cxt) {
      const rule = portableRules[cxt.schema];
      if (!rule) throw new Error(`unknown portable rule: ${cxt.schema}`);
      const fn = cxt.gen.scopeValue('func', { ref: rule, code: new _Code(rule.toString()) });
      cxt.fail(_`!${fn}(${cxt.data})`);
    },
  });
  ajv.compile(schema);
  const source = standaloneCode(ajv, { validateWire: schema.$id });
  // Bundle Ajv's tiny generated-code helpers; no external import/compiler remains.
  // Its ucs2length helper retains an inert `.code` string mentioning require.
  const result = buildSync({
    stdin: { contents: source, resolveDir: OUT_DIR, sourcefile: 'wire-validator.generated.mjs' },
    bundle: true, platform: 'browser', format: 'esm', target: 'es2022',
    write: false, legalComments: 'inline', minify: true,
    banner: { js: '// GENERATED from canonical Rust wire schema; DO NOT EDIT.\n// Ajv 8.20.0 (MIT); regenerate with tools/contracts/generate.mjs.' },
  }).outputFiles[0].text;
  if (/\beval\s*\(|\bnew\s+Function\b|\bnode:/.test(result)) {
    throw new Error('generated validator violates the browser runtime boundary: ' + result.match(/.{0,60}(?:\beval\s*\(|\bnew\s+Function\b|\bnode:).{0,60}/)?.[0]);
  }
  return result;
}

/** Run a Rust example through the managed toolchain. */
function cargo(args) {
  return execFileSync(DEV_ENV, ['cargo', ...args], {
    cwd: ROOT,
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
  });
}

/** Split a ts-rs file into its single exported type declaration. */
function extractBody(source) {
  const lines = source.split('\n');
  const start = lines.findIndex((line) => line.startsWith('export type'));
  if (start === -1) throw new Error('no exported type found');
  return lines
    .slice(start)
    .join('\n')
    .trimEnd();
}

function exportNameOf(source) {
  const match = /^export type ([A-Za-z0-9_]+)/m.exec(source);
  if (!match) throw new Error('cannot read export name');
  return match[1];
}

function main() {
  const scratch = mkdtempSync(join(tmpdir(), 'avencrew-contracts-'));
  try {
    // 1. Export from Rust. These are the existing reviewed examples.
    cargo(['run', '-q', '-p', 'avencrew-contracts', '--example', 'export_typescript',
      '--features', 'typescript-export', '--locked', '--', scratch]);
    const schemaJson = cargo(['run', '-q', '-p', 'avencrew-contracts', '--example', 'export_schema',
      '--features', 'schema-export', '--locked']);

    // 2. Bundle the per-type modules into one file, ordered by export name.
    const modules = readdirSync(scratch)
      .filter((name) => name.endsWith('.ts'))
      .map((name) => ({ name, source: readFileSync(join(scratch, name), 'utf8') }));

    const seen = new Set();
    const ordered = modules
      .map((module) => ({ body: extractBody(module.source), exported: exportNameOf(module.source) }))
      .sort((a, b) => (a.exported < b.exported ? -1 : a.exported > b.exported ? 1 : 0));
    for (const entry of ordered) {
      if (seen.has(entry.exported)) throw new Error(`duplicate export name ${entry.exported}`);
      seen.add(entry.exported);
    }

    const preamble = [
      '// GENERATED FILE - DO NOT EDIT.',
      '//',
      '// Source of truth: crates/contracts (Rust). Regenerate with:',
      '//   node tools/contracts/generate.mjs',
      '// Verify freshness with:',
      '//   node tools/contracts/generate.mjs --check',
      '//',
      '// Runtime validation uses the generated wire-validator.mjs through validate.ts.',
      '// Types alone cannot reject invalid fields, duplicate JSON keys or dates.',
      '',
    ].join('\n');

    const wire = `${preamble}${ordered.map((entry) => `\n${entry.body}\n`).join('')}`;

    // 3. Compare or write.
    if (!CHECK_ONLY) mkdirSync(SRC_DIR, { recursive: true });
    // Mirror the canonical synthetic examples so the package never reaches into
    // crates/ (or any gitignored path) at check time.
    if (!CHECK_ONLY) mkdirSync(FIXTURES_DIR, { recursive: true });
    const wireExamples = readFileSync(WIRE_EXAMPLES, 'utf8');
    const targets = [
      [WIRE_TS, wire],
      [join(SRC_DIR, 'wire-validator.mjs'), validatorSource(schemaJson)],
      [SCHEMA_JSON, schemaJson.endsWith('\n') ? schemaJson : `${schemaJson}\n`],
      [WIRE_EXAMPLES_COPY, wireExamples],
    ];

    if (CHECK_ONLY) {
      const stale = [];
      for (const [path, content] of targets) {
        let current;
        try {
          current = readFileSync(path, 'utf8');
        } catch {
          stale.push(`${path} is missing`);
          continue;
        }
        if (current !== content) stale.push(`${path} differs from canonical Rust output`);
      }
      if (stale.length > 0) {
        console.error('contract artifacts are stale:');
        for (const line of stale) console.error(`  - ${line}`);
        console.error('run: node tools/contracts/generate.mjs');
        process.exitCode = 1;
        return;
      }
      console.log(`contract artifacts up to date (${ordered.length} types)`);
      return;
    }

    for (const [path, content] of targets) writeFileSync(path, content);
    console.log(`wrote ${WIRE_TS.replace(ROOT + '/', '')} (${ordered.length} types)`);
    console.log(`wrote ${SCHEMA_JSON.replace(ROOT + '/', '')}`);
    console.log('wrote packages/contracts-ts/src/wire-validator.mjs (CSP-safe runtime)');
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

main();