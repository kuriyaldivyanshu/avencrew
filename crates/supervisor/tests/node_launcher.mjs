// Synthetic trusted-main test. Credentials stay in inherited pipes and memory.
import { spawn } from 'node:child_process';
import { createHmac } from 'node:crypto';
import { connect } from 'node:net';
import { once } from 'node:events';
const id = n => `01900000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;
const child = spawn(process.argv[1], ['serve', '--data-root', process.argv[2]], { stdio: ['pipe', 'pipe', 'pipe'] });
let socket;
const timer = setTimeout(() => { child.kill('SIGKILL'); process.exitCode = 1; socket?.destroy(); }, 10000);
const canonical = value => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map(k => `${JSON.stringify(k)}:${canonical(value[k])}`).join(',')}}`;
  return JSON.stringify(value);
};
function frame(value) {
  const bytes = Buffer.from(JSON.stringify(value));
  const header = Buffer.alloc(4); header.writeUInt32BE(bytes.length);
  return Buffer.concat([header, bytes]);
}
function reader(stream) {
  let pending = Buffer.alloc(0), waiting, queued = [];
  stream.on('data', bytes => {
    pending = Buffer.concat([pending, bytes]);
    while (pending.length >= 4 && pending.length >= 4 + pending.readUInt32BE()) {
      const n = pending.readUInt32BE();
      if (n === 0 || n > 1048576) throw Error('invalid frame');
      const value = JSON.parse(pending.subarray(4, 4+n)); pending = pending.subarray(4+n);
      if (waiting) { const resolve=waiting; waiting=undefined; resolve(value); } else queued.push(value);
    }
  });
  return () => queued.length ? Promise.resolve(queued.shift()) : new Promise(resolve => { waiting=resolve; });
}
let stderr='';child.stderr.on('data', b => { stderr += b; });
try {
  const nextGrant=reader(child.stdout);
  child.stdin.write(frame({schema_version:'avencrew.local-launch/1',workspace_id:id(1),actor_id:id(2),main_pid:process.pid,client_build_digest:'a'.repeat(64)}));
  const grant=await nextGrant();child.stdin.end();
  socket=connect(grant.socket_path); const next=reader(socket);await once(socket,'connect');
  const payload={frame_kind:'request',operation:'handshake',request_id:id(120),body:{supported_versions:['1.0'],client_kind:'electron_main',build_digest:'a'.repeat(64),nonce:'c'.repeat(64),challenge_response:'',requested_workspace_id:id(1)}};
  const transcript={domain:'avencrew.local-handshake/1',challenge:grant.challenge,generation:grant.generation,scope_id:grant.scope_id,peer_uid:process.geteuid(),peer_pid:process.pid,request:payload};
  payload.body.challenge_response=createHmac('sha256',Buffer.from(grant.session_key,'hex')).update(canonical(transcript)).digest('hex');
  socket.write(frame({message_kind:'rpc_request',schema_version:'1.0',payload}));
  if ((await next()).payload.ok !== true) throw Error('handshake failed');
  socket.write(frame({message_kind:'rpc_request',schema_version:'1.0',payload:{frame_kind:'request',operation:'describe_capabilities',request_id:id(121),body:{}}}));
  if ((await next()).payload.result.protocol_version !== '1.0') throw Error('capabilities failed');
  socket.destroy();child.kill('SIGKILL');await once(child,'close');
  if (stderr) throw Error('unexpected supervisor stderr');
  console.log('node launch and proof verified');
} catch {
  // Never print the grant, proof, environment or raw protocol on failure.
  console.error('node launch/proof failed');process.exitCode=1;
} finally {
  clearTimeout(timer);socket?.destroy();child.kill('SIGKILL');
}
