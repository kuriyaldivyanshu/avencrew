import assert from 'node:assert/strict';
import test from 'node:test';
import { PassThrough } from 'node:stream';
import {
  FrameReader,
  frame,
  writeFrame,
  endStream,
  RejectedError,
} from '../dist/tests/transport.js';
import { SupervisorClient, request } from '../dist/tests/client.js';
console.log(
  JSON.stringify({ check: 'desktop-unit', node: process.version, executable: process.execPath }),
);
const id = (n) => `01900000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;
const caps = {
  protocol_version: '1.0',
  build_digest: 'a'.repeat(64),
  schema_versions: ['sqlite:L0-L2'],
  operation_names: ['handshake', 'describe_capabilities', 'submit_command'],
  profiles: [],
  capability_manifest_digest: 'b'.repeat(64),
};
function raw(text) {
  const b = Buffer.from(text);
  const h = Buffer.alloc(4);
  h.writeUInt32BE(b.length);
  return Buffer.concat([h, b]);
}
function readerTest(bytes) {
  const stream = new PassThrough();
  let failures = 0;
  const values = [];
  const reader = new FrameReader(
    stream,
    (v) => values.push(v),
    () => failures++,
  );
  stream.write(bytes);
  return {
    stream,
    reader,
    values,
    get failures() {
      return failures;
    },
  };
}
test('fragmented and coalesced strict frames', () => {
  const stream = new PassThrough();
  const values = [];
  const reader = new FrameReader(
    stream,
    (v) => values.push(v),
    () => assert.fail(),
  );
  const b = Buffer.concat([frame({ safe: true }), frame('challenge')]);
  for (const byte of b) stream.write(Buffer.from([byte]));
  assert.deepEqual(values, [{ safe: true }, 'challenge']);
  reader.close();
  stream.destroy();
});
test('zero/oversize/duplicate/malformed/invalid UTF8 reject and release', () => {
  const oversize = Buffer.alloc(4);
  oversize.writeUInt32BE(1048577);
  const invalid = Buffer.from([0, 0, 0, 2, 0xc3, 0x28]);
  for (const bytes of [Buffer.alloc(4), oversize, raw('{"x":1,"x":2}'), raw('{'), invalid]) {
    const t = readerTest(bytes);
    assert.equal(t.failures, 1);
    assert.deepEqual(t.values, []);
    assert.equal(t.reader.buffer.length, 0);
    t.stream.destroy();
  }
});
test('partial frame real five-second deadline', async () => {
  const header = Buffer.alloc(4);
  header.writeUInt32BE(1048576);
  const t = readerTest(header);
  const start = performance.now();
  const drip = setInterval(() => t.stream.write(Buffer.from([0])), 1000);
  await new Promise((r) => setTimeout(r, 5200));
  clearInterval(drip);
  assert(performance.now() - start >= 4900);
  assert.equal(t.failures, 1);
  assert.equal(t.reader.buffer.length, 0);
  t.stream.destroy();
});
function clientPair() {
  const client = new SupervisorClient();
  const stream = new PassThrough();
  client.socket = stream;
  client.ready = true;
  return { client, stream };
}
const response = (request, result = caps) => ({
  message_kind: 'rpc_response',
  schema_version: '1.0',
  payload: {
    frame_kind: 'response',
    request_id: request.payload.request_id,
    operation: request.payload.operation,
    ok: true,
    result,
  },
});
test('valid correlation returns exact payload; no optimistic acceptance', async () => {
  const { client, stream } = clientPair();
  const msg = request('describe_capabilities', {}, id(1));
  const result = client.send(msg);
  client.reply(response(msg));
  assert.deepEqual(await result, caps);
  assert.equal(client.pending.size, 0);
  client.disconnect();
  stream.destroy();
});
test('wrong request id/operation/internal/unknown/schema rejects every pending', async () => {
  for (const mutate of [
    (v) => {
      v.payload.request_id = id(99);
    },
    (v) => {
      v.payload.operation = 'submit_command';
    },
    (v) => {
      v.message_kind = 'rpc_event';
    },
    (v) => {
      v.extra = true;
    },
    (v) => {
      v.schema_version = '2.0';
    },
  ]) {
    const { client, stream } = clientPair();
    const msg = request('describe_capabilities', {}, id(1));
    const a = client.send(msg);
    const b = client.send(request('describe_capabilities', {}, id(2)));
    const assertions = [
      assert.rejects(a, /Supervisor unavailable/),
      assert.rejects(b, /Supervisor unavailable/),
    ];
    const reply = response(msg);
    mutate(reply);
    client.reply(reply);
    await Promise.all(assertions);
    assert.equal(client.pending.size, 0);
    assert.equal(client.socket, undefined);
    stream.destroy();
  }
});
test('pending capacity bounded to sixteen; disconnect rejects all', async () => {
  const { client, stream } = clientPair();
  const pending = Array.from({ length: 16 }, (_, n) =>
    client.send(request('describe_capabilities', {}, id(n + 1))),
  );
  await assert.rejects(client.send(request('describe_capabilities', {}, id(99))));
  assert.equal(client.pending.size, 16);
  const checked = pending.map((p) => assert.rejects(p));
  client.disconnect();
  await Promise.all(checked);
  assert.equal(client.pending.size, 0);
  stream.destroy();
});
test('real ten-second request deadline disconnects and rejects all', async () => {
  const { client, stream } = clientPair();
  const start = performance.now();
  await assert.rejects(client.send(request('describe_capabilities', {}, id(1))));
  assert(performance.now() - start >= 9900);
  assert.equal(client.failed, true);
  stream.destroy();
});
test('real five-second blocked write deadline destroys stream', async () => {
  const stream = new PassThrough({ highWaterMark: 1 });
  const start = performance.now();
  await assert.rejects(writeFrame(stream, { data: 'x'.repeat(10000) }));
  assert(performance.now() - start >= 4900);
  assert.equal(stream.destroyed, true);
});

test('server inbound EOF rejects a genuinely outstanding request', async () => {
  const { client, stream: outbound } = clientPair();
  const inbound = new PassThrough();
  let failures = 0;
  client.reader = new FrameReader(
    inbound,
    (value) => client.reply(value),
    () => {
      failures++;
      client.disconnect();
    },
  );
  const result = client.send(request('describe_capabilities', {}, id(1)));
  assert.equal(client.pending.size, 1);
  assert.equal(failures, 0);
  const rejected = assert.rejects(result, /Supervisor unavailable/);
  inbound.end();
  await new Promise(setImmediate);
  assert.equal(client.pending.size, 0);
  assert.equal(failures, 1);
  await rejected;
  assert.equal(failures, 1);
  assert.equal(client.pending.size, 0);
  assert.equal(outbound.destroyed, true);
  inbound.destroy();
});

test('launch grant/authentication fail closed: exact fields/path, extra grant, scope/generation, real deadlines', async () => {
  const { mkdtemp, mkdir, readFile, writeFile, rm, realpath } = await import('node:fs/promises');
  const { tmpdir } = await import('node:os');
  for (const mode of [
    'extra-grant',
    'grant-array',
    'grant-newline',
    'grant-path',
    'two-grants',
    'generation',
    'scope',
    'version',
    'schema',
    'launch-timeout',
    'auth-timeout',
  ]) {
    const root = await realpath(await mkdtemp(`${tmpdir()}/av-peer-`));
    await mkdir(`${root}/control`);
    const source = await readFile(new URL('./peer.mjs', import.meta.url), 'utf8');
    const executable = `${root}/peer`;
    await writeFile(executable, `#!${process.execPath}\n${source}`, { mode: 0o700 });
    await writeFile(`${root}/mode`, mode);
    try {
      const start = performance.now();
      await assert.rejects(
        SupervisorClient.launch({
          executable,
          dataRoot: root,
          workspaceId: id(1),
          actorId: id(2),
          buildDigest: 'a'.repeat(64),
        }),
        /Supervisor unavailable/,
      );
      if (mode.endsWith('timeout')) assert(performance.now() - start >= 4900);
      if (['generation', 'scope', 'version', 'schema', 'auth-timeout'].includes(mode))
        assert.equal(await readFile(`${root}/stage`, 'utf8'), 'handshake');
      console.log(await readFile(`${root}/runtime.json`, 'utf8'));
      const failedPid = Number(await readFile(`${root}/pid`, 'utf8'));
      assert.throws(() => process.kill(failedPid, 0), { code: 'ESRCH' });
    } finally {
      // Explicit teardown of this test-owned peer, never normal disconnect policy.
      const pid = Number(await readFile(`${root}/pid`, 'utf8'));
      try {
        process.kill(pid, 'SIGKILL');
      } catch {}
      await rm(root, { recursive: true, force: true });
    }
  }
});

test('maximum-size fragmented frame uses one bounded payload allocation; close is idempotent', () => {
  const stream = new PassThrough();
  const values = [];
  const reader = new FrameReader(
    stream,
    (value) => values.push(value),
    () => assert.fail(),
  );
  const value = 'x'.repeat(1048574);
  const encoded = frame(value);
  stream.write(encoded.subarray(0, 4));
  const allocation = reader.buffer;
  assert.equal(allocation.length, 1048576);
  for (let at = 4; at < encoded.length; at += 17) {
    stream.write(encoded.subarray(at, Math.min(at + 17, encoded.length)));
    if (at + 17 < encoded.length) assert.equal(reader.buffer, allocation);
  }
  assert.deepEqual(values, [value]);
  reader.close();
  const listeners = stream.listenerCount('error');
  for (let n = 0; n < 100; n++) reader.close();
  assert.equal(stream.listenerCount('error'), listeners);
  assert.equal(stream.listenerCount('data'), 0);
  stream.destroy();
});

test('stdin write and asynchronous end errors reject safely', async () => {
  const { Writable } = await import('node:stream');
  for (const operation of ['write', 'end']) {
    const stream = new Writable({
      write(_bytes, _encoding, done) {
        done(operation === 'write' ? new Error('fixture pipe failure') : undefined);
      },
      final(done) {
        done(new Error('fixture end failure'));
      },
    });
    await assert.rejects(
      operation === 'write' ? writeFrame(stream, { safe: true }) : endStream(stream),
      /Supervisor unavailable/,
    );
  }
});

test('valid server rejection is distinguished from transport failure', async () => {
  const { client, stream } = clientPair();
  const msg = request('describe_capabilities', {}, id(1));
  const rejected = assert.rejects(client.send(msg), (error) => error instanceof RejectedError);
  client.reply({
    message_kind: 'rpc_response',
    schema_version: '1.0',
    payload: {
      frame_kind: 'response',
      request_id: msg.payload.request_id,
      operation: 'describe_capabilities',
      ok: false,
      error: { code: 'UNSUPPORTED', message: 'untrusted text', evidence_refs: [] },
    },
  });
  await rejected;
  assert.equal(client.failed, false);
  client.disconnect();
  stream.destroy();
});

test('schema-valid command receipts must bind the submitted command and run', async () => {
  const body = {
    kind: 'stop',
    command_id: id(400),
    run_id: id(300),
    idempotency_key: 'receipt-binding',
    payload: { reason: 'stop' },
  };
  const receipt = {
    kind: 'accepted',
    command_id: body.command_id,
    run_id: body.run_id,
    authority: 'local',
    accepted_seq: '1',
    event_seq: '1',
    row_version: '1',
    disposition_ref: null,
    task_id: id(100),
    task_revision_id: id(103),
  };
  for (const wrong of [
    { ...receipt, command_id: id(401) },
    { ...receipt, run_id: id(301) },
  ]) {
    const { client, stream } = clientPair();
    client.send = async () => wrong;
    await assert.rejects(client.command(body), /Supervisor unavailable/);
    assert.equal(client.failed, true);
    stream.destroy();
  }
  const { client, stream } = clientPair();
  client.send = async () => receipt;
  assert.equal(await client.command(body), receipt);
  client.disconnect();
  stream.destroy();
});

test('immediate child exit is a rejected launch, with no uncaught stdin error', async () => {
  const { mkdtemp, writeFile, rm, realpath } = await import('node:fs/promises');
  const root = await realpath(await mkdtemp('/tmp/av-early-'));
  try {
    const executable = `${root}/exit`;
    await writeFile(executable, '#!/bin/sh\nexit 1\n', { mode: 0o700 });
    await assert.rejects(
      SupervisorClient.launch({
        executable,
        dataRoot: root,
        workspaceId: id(1),
        actorId: id(2),
        buildDigest: 'a'.repeat(64),
      }),
      /Supervisor unavailable/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
