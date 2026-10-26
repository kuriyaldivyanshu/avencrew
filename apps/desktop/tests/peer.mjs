// Malformed synthetic peer for client fail-closed checks, never production code.
import { createServer } from 'node:net';
import { readFileSync, writeFileSync } from 'node:fs';
const root = process.argv[4];
const mode = readFileSync(`${root}/mode`, 'utf8');
writeFileSync(`${root}/pid`, String(process.pid));
writeFileSync(
  `${root}/runtime.json`,
  JSON.stringify({
    check: 'desktop-test-peer',
    node: process.version,
    executable: process.execPath,
    mode,
  }),
);
const id = '01900000-0000-7000-8000-000000000010';
const frame = (value) => {
  const b = Buffer.from(JSON.stringify(value));
  const h = Buffer.alloc(4);
  h.writeUInt32BE(b.length);
  return Buffer.concat([h, b]);
};
const grant = {
  schema_version: 'avencrew.local-session/1',
  socket_path: `${root}/control/sock`,
  generation: '1',
  scope_id: id,
  challenge: 'a'.repeat(64),
  session_key: 'b'.repeat(64),
};
process.stdin.resume();
process.stdin.on('error', () => {});
process.stdout.on('error', () => {});
setTimeout(() => process.exit(0), 15000);
if (mode === 'launch-timeout') {
} else {
  const server = createServer((socket) => {
    socket.on('error', () => {});
    socket.write(frame('c'.repeat(64)));
    let buffer = Buffer.alloc(0);
    socket.on('data', (b) => {
      buffer = Buffer.concat([buffer, b]);
      if (buffer.length < 4 || buffer.length < buffer.readUInt32BE() + 4) return;
      const req = JSON.parse(buffer.subarray(4));
      buffer = Buffer.alloc(0);
      writeFileSync(`${root}/stage`, 'handshake');
      if (mode === 'auth-timeout') return;
      const result = {
        selected_version: mode === 'version' ? '2.0' : '1.0',
        supervisor_generation: mode === 'generation' ? '2' : '1',
        schema_min: mode === 'schema' ? 'incompatible' : 'sqlite:L0-L2',
        schema_max: 'sqlite:L0-L2',
        capability_manifest_digest: 'a'.repeat(64),
        max_frame_bytes: 1048576,
        authenticated_scope_id: mode === 'scope' ? '01900000-0000-7000-8000-000000000011' : id,
      };
      socket.write(
        frame({
          message_kind: 'rpc_response',
          schema_version: '1.0',
          payload: {
            frame_kind: 'response',
            operation: 'handshake',
            request_id: req.payload.request_id,
            ok: true,
            result,
          },
        }),
      );
    });
  });
  server.listen(grant.socket_path, () => {
    if (mode === 'extra-grant') grant.extra = true;
    if (mode === 'grant-array') grant.session_key = ['b'.repeat(64)];
    if (mode === 'grant-newline') grant.challenge += '\n';
    if (mode === 'grant-path') grant.socket_path = `${root}/elsewhere`;
    process.stdout.write(
      Buffer.concat(mode === 'two-grants' ? [frame(grant), frame(grant)] : [frame(grant)]),
    );
  });
}
