// Fixture integrity only. Business acceptance oracles remain integrator-owned.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { lstatSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { validateWire } from '../../packages/contracts-ts/src/wire-validator.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../fixtures');
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const load = (path) => JSON.parse(readFileSync(join(root, path), 'utf8'));
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u;

function inventory(directory, prefix = '') {
  assert(lstatSync(directory).isDirectory(), 'fixture root must be a real directory');
  return readdirSync(directory).flatMap((name) => {
    const path = join(directory, name);
    const relative = prefix + name;
    const stat = lstatSync(path);
    assert(!stat.isSymbolicLink(), 'fixture symlinks are forbidden');
    if (stat.isDirectory()) return inventory(path, `${relative}/`);
    assert(stat.isFile(), 'fixture special files are forbidden');
    return relative === 'manifest.json' ? [] : [relative];
  });
}

function stable(value) {
  if (Array.isArray(value)) return value.map(stable);
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, stable(value[key])]),
    );
  }
  return value;
}

function unique(values, label) {
  assert.equal(new Set(values).size, values.length, `${label} must be unique`);
}

const manifest = load('manifest.json');
assert.equal(manifest.schema_version, 1);
assert.equal(manifest.kind, 'avencrew-application-fixture-inventory');
unique(
  manifest.files.map((file) => file.path),
  'inventory paths',
);
const paths = manifest.files.map((file) => file.path);
assert.deepEqual(paths, inventory(root).sort(), 'inventory must cover exactly the fixture files');
for (const file of manifest.files) {
  assert(
    /^(models|sources|projects|faults)\/[a-zA-Z0-9_./-]+$|^(README\.md|ids\.json)$/u.test(
      file.path,
    ),
  );
  assert(!file.path.split('/').some((part) => part === '.' || part === '..'));
  const bytes = readFileSync(join(root, file.path));
  assert.equal(bytes.length, file.bytes, `byte length: ${file.path}`);
  assert.equal(sha256(bytes), file.sha256, `digest: ${file.path}`);
}
const identities = load('ids.json');
assert.equal(identities.synthetic, true);
const ids = identities.ids;
unique(Object.values(ids), 'identity IDs');
for (const id of Object.values(ids)) assert(uuid.test(id));
assert.notEqual(ids.workspace, ids.other_workspace);

const sources = load('sources/evidence.json').sources;
unique(
  sources.map((source) => source.name),
  'source names',
);
const sourceByName = new Map(sources.map((source) => [source.name, source]));
const allIds = [...Object.values(ids)];
for (const source of sources) {
  assert([ids.workspace, ids.other_workspace].includes(source.workspace_id));
  allIds.push(source.resource_id, source.version_id);
  assert(Object.values(ids).includes(source.author_id));
  assert(source.audience_ids.length > 0);
  unique(source.audience_ids, 'audience IDs');
  for (const id of source.audience_ids) assert(Object.values(ids).includes(id));
  assert(source.covered_range.from < source.covered_range.to_exclusive);
  assert.equal(source.permitted_query_scope, source.name);
  assert(Array.isArray(source.omissions));
  assert.equal(sha256(JSON.stringify(stable(source.content))), source.content_sha256);
  const visited = new Set();
  let current = source;
  while (current.lineage?.forward_of) {
    assert(!visited.has(current.name), 'lineage cycle');
    visited.add(current.name);
    const parent = sourceByName.get(current.lineage.forward_of);
    assert(parent, 'lineage parent missing');
    assert.equal(parent.workspace_id, source.workspace_id);
    assert(parent.lineage, `lineage parent has no lineage: ${parent.name}`);
    assert.equal(parent.lineage.origin, source.lineage.origin);
    current = parent;
  }
}
unique(allIds, 'allocated fixture IDs');
for (const id of allIds) assert(uuid.test(id));
const projects = load('projects/scenarios.json');
assert.deepEqual(
  projects.scenarios.map((scenario) => scenario.id),
  ['F01', 'F02', 'F03', 'F04', 'F05'],
);
for (const scenario of projects.scenarios) {
  assert.equal(scenario.workspace_id, ids.workspace);
  assert.equal(scenario.project_id, ids.project);
  assert.equal(scenario.external_effects_enabled, false);
  for (const name of scenario.source_names) {
    assert.equal(sourceByName.get(name)?.workspace_id, scenario.workspace_id);
  }
  scenario.events.forEach((event, index) => {
    assert.equal(event.sequence, String(index + 1));
  });
}
const faults = load('faults/injections.json').faults;
unique(
  faults.map((fault) => fault.id),
  'fault IDs',
);
for (const fault of faults) assert(fault.boundary && fault.action && fault.orders.length > 0);
const models = load('models/responses.json');
assert.equal(models.profile.rate_usd, '0.000000');
assert.equal(models.profile.network_enabled, false);
assert.equal(models.profile.missing_usage, 'unknown');
unique(
  models.responses.map((response) => response.name),
  'model response names',
);
for (const response of models.responses) {
  assert(validateWire(response.message), `invalid normalized wire fixture: ${response.name}`);
  assert.equal(response.message.payload.model_call_id, ids.model_call);
  assert.equal(response.message.payload.step_id, ids.step);
}
console.log(
  `Fixture integrity passed: ${paths.length} files, ${sources.length} sources, 5 journeys, ${faults.length} fault inputs, ${models.responses.length} wire responses.`,
);
console.log(
  'No harness, model, tool, database or fault injection ran; runtime acceptance remains unverified.',
);
