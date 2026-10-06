/**
 * Fixture-only seed loader for local development.
 *
 * Copies the committed synthetic fixtures into an isolated data root and
 * writes a small manifest describing what landed there. Nothing else.
 *
 * What this deliberately does NOT do, because those are later phases:
 *   - create or migrate any database (no journal schema exists yet)
 *   - run a model, a harness, a supervisor or any tool
 *   - claim any task, run or receipt succeeded
 *   - read an API key, contact a network, or touch a paid service
 *
 * It is a file copy plus a manifest. If a future phase needs real seed
 * behaviour, that phase owns it.
 *
 * Usage:
 *   node tools/dev/seed.mjs [--data-root DIR] [--print-root]
 *
 * --data-root defaults to a fresh directory under the OS temp dir.
 */

import { createHash } from 'node:crypto';
import { lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

/**
 * Synthetic fixtures, copied verbatim. Both are committed, not generated into
 * existence by this loader, so seeding needs no network and no build step.
 *
 * The contract corpus is the same file the Rust and TypeScript conformance
 * suites read. Seed hashes describe the copied bytes; conformance checks the
 * canonical Rust source corpus separately.
 */
const FIXTURE_SOURCES = [
  'packages/contracts-ts/fixtures/wire-examples.json',
  'packages/contracts-ts/fixtures/fixture-manifest.json',
];

/** Written into every seeded root so an accidental real-data root is obvious. */
const MARKER = '.avencrew-fixture-root';

// Resolve existing ancestors before checking boundaries. Only missing paths are
// tolerated; permission failures and malformed paths must fail closed.
function canonicalPath(candidate) {
  const absolute = resolve(candidate);
  try {
    return realpathSync(absolute);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    const parent = dirname(absolute);
    if (parent === absolute) throw error;
    return join(canonicalPath(parent), relative(parent, absolute));
  }
}

function within(candidate, root) {
  return candidate === root || candidate.startsWith(root + sep);
}

function statIfPresent(path) {
  try {
    return lstatSync(path);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    return undefined;
  }
}

function assertSafeDataRoot(candidate) {
  const original = resolve(candidate);
  if (statIfPresent(original)?.isSymbolicLink()) {
    throw new Error('refusing a symlink data root');
  }
  const resolved = canonicalPath(original);
  const temporary = realpathSync(tmpdir());
  const forbidden = [REPO_ROOT, homedir(), process.env.APPDATA]
    .filter(Boolean).map(canonicalPath);
  if (forbidden.some((root) => within(resolved, root))) {
    throw new Error(`refusing a real application data root: ${resolved}`);
  }
  // No override: this lane only needs disposable temporary subdirectories.
  if (resolved === temporary || !within(resolved, temporary)) {
    throw new Error(`data root must be a subdirectory of ${temporary}`);
  }
  return resolved;
}

/** Prepare all bytes before touching the destination. */
function seed(dataRoot) {
  const contents = new Map();
  const files = FIXTURE_SOURCES.map((source) => {
    const bytes = readFileSync(join(REPO_ROOT, source));
    const path = `fixtures/${source.split('/').pop()}`;
    contents.set(path, bytes);
    return { path, source, bytes: bytes.length,
      sha256: createHash('sha256').update(bytes).digest('hex') };
  }).sort((a, b) => a.path.localeCompare(b.path, 'en'));
  const manifest = {
    kind: 'avencrew-fixture-seed', schema_version: '1',
    note: 'Synthetic development fixtures. Not real data, not a database, and not evidence that any run succeeded.',
    files,
  };
  contents.set('manifest.json', Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`));
  contents.set(MARKER, Buffer.from(
    'This directory holds synthetic Avencrew development fixtures.\n' +
    'It is not application data. Delete it freely.\n'));

  const rootStat = statIfPresent(dataRoot);
  if (rootStat && !rootStat.isDirectory()) throw new Error('data root is not a directory');
  if (rootStat && readdirSync(dataRoot).length > 0) {
    // Reuse only a complete, byte-identical seed. Reject unmarked directories,
    // symlinks, extra entries and changed files before making any writes.
    const expectedEntries = [...contents.keys()].sort();
    const actualEntries = [];
    for (const name of readdirSync(dataRoot)) {
      const path = join(dataRoot, name);
      const stat = lstatSync(path);
      if (name === 'fixtures' && stat.isDirectory()) {
        for (const child of readdirSync(path)) actualEntries.push(`fixtures/${child}`);
      } else {
        actualEntries.push(name);
      }
    }
    if (JSON.stringify(actualEntries.sort()) !== JSON.stringify(expectedEntries)) {
      throw new Error('refusing an existing directory containing unrelated or incomplete data');
    }
    for (const [path, bytes] of contents) {
      const target = join(dataRoot, path);
      if (!lstatSync(target).isFile() || !readFileSync(target).equals(bytes)) {
        throw new Error(`refusing changed or symlinked seed file: ${path}`);
      }
    }
    return manifest;
  }

  mkdirSync(join(dataRoot, 'fixtures'), { recursive: true });
  for (const [path, bytes] of contents) {
    // Exclusive creation also refuses pre-existing files and symlinks.
    writeFileSync(join(dataRoot, path), bytes, { flag: 'wx' });
  }
  return manifest;
}

// ---- entry point ------------------------------------------------------------

const argv = process.argv.slice(2);
const dataRootIndex = argv.indexOf('--data-root');
const printRoot = argv.includes('--print-root');

let dataRoot;
if (dataRootIndex !== -1) {
  const value = argv[dataRootIndex + 1];
  if (value === undefined) {
    console.error('seed: --data-root needs a directory');
    process.exit(2);
  }
  dataRoot = value;
} else {
  // Unique per run so two isolated roots never share state by accident.
  dataRoot = mkdtempSync(join(tmpdir(), 'avencrew-fixture-root-'));
}

try {
  const safeRoot = assertSafeDataRoot(dataRoot);
  const manifest = seed(safeRoot);

  if (printRoot) {
    console.log(safeRoot);
  } else {
    console.log(`seeded ${manifest.files.length} fixture file(s) into ${safeRoot}`);
    for (const file of manifest.files) {
      console.log(`  ${file.path}  ${file.bytes} bytes  sha256:${file.sha256.slice(0, 12)}`);
    }
    console.log('no database was created, no model ran, and no network was used');
  }
} catch (error) {
  console.error(`seed: ${error.message}`);
  process.exit(1);
}
