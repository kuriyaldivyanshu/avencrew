import { type ChildProcessWithoutNullStreams, spawn } from 'node:child_process';
import { createHmac, randomBytes } from 'node:crypto';
import { connect, type Socket } from 'node:net';
import { isAbsolute, join } from 'node:path';
import {
  parseWireMessage,
  validateCommandReceiptPayload,
  validateCounter,
  validateMessage,
} from '@avencrew/contracts-ts/validate';
import type {
  CapabilityDescription,
  CommandReceipt,
  RpcRequestSubmitCommandBody,
} from '@avencrew/contracts-ts/wire';
import { BoundaryError, FrameReader, IO_DEADLINE, writeFrame } from './transport.js';

export interface TrustedLaunch {
  readonly executable: string;
  readonly dataRoot: string;
  readonly workspaceId: string;
  readonly actorId: string;
  readonly buildDigest: string;
}
type Grant = {
  schema_version: string;
  socket_path: string;
  generation: string;
  scope_id: string;
  challenge: string;
  session_key: string;
};
type Reply = {
  frame_kind: string;
  request_id: string;
  operation: string;
  ok: boolean;
  result?: unknown;
};
type Pending = {
  operation: string;
  resolve: (value: unknown) => void;
  reject: (error: BoundaryError) => void;
  timer: ReturnType<typeof setTimeout>;
};
const HEX = /^[0-9a-f]{64}$(?![\s\S])/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$(?![\s\S])/;
function id(): string {
  const bytes = randomBytes(16);
  bytes.writeUIntBE(Date.now(), 0, 6);
  bytes[6] = ((bytes[6] ?? 0) & 15) | 112;
  bytes[8] = ((bytes[8] ?? 0) & 63) | 128;
  const h = bytes.toString('hex');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}
function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object')
    return `{${Object.entries(value)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([k, v]) => `${JSON.stringify(k)}:${canonical(v)}`)
      .join(',')}}`;
  return JSON.stringify(value);
}
export function request(operation: string, body: unknown, requestId = id()) {
  return {
    message_kind: 'rpc_request',
    schema_version: '1.0',
    payload: { frame_kind: 'request', operation, request_id: requestId, body },
  };
}
function grantValue(value: unknown, config: TrustedLaunch): Grant {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new BoundaryError();
  const v = value as Grant;
  if (
    Object.keys(v).sort().join(',') !==
      'challenge,generation,schema_version,scope_id,session_key,socket_path' ||
    !Object.values(v).every((field) => typeof field === 'string') ||
    v.schema_version !== 'avencrew.local-session/1' ||
    v.socket_path !== join(config.dataRoot, 'control', 'sock') ||
    !validateCounter(v.generation, true).ok ||
    !UUID.test(v.scope_id) ||
    !HEX.test(v.challenge) ||
    !HEX.test(v.session_key)
  )
    throw new BoundaryError();
  return v;
}
/** Only main owns this object. Disconnect never signals or cancels the supervisor. */
export class SupervisorClient {
  private socket: Socket | undefined;
  private reader: FrameReader | undefined;
  private grantReader: FrameReader | undefined;
  private manifest: string | undefined;
  private child: ChildProcessWithoutNullStreams | undefined;
  private pending = new Map<string, Pending>();
  private ready = false;
  private failed = false;
  private startupReject: (() => void) | undefined;
  static async launch(config: TrustedLaunch): Promise<SupervisorClient> {
    const client = new SupervisorClient();
    try {
      await client.start(config);
      return client;
    } catch {
      client.disconnect();
      throw new BoundaryError();
    }
  }
  private async start(config: TrustedLaunch): Promise<void> {
    if (
      !isAbsolute(config.executable) ||
      !isAbsolute(config.dataRoot) ||
      !UUID.test(config.workspaceId) ||
      !UUID.test(config.actorId) ||
      !HEX.test(config.buildDigest)
    )
      throw new BoundaryError();
    const child = spawn(config.executable, ['serve', '--data-root', config.dataRoot], {
      shell: false,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    this.child = child;
    // Drain without retaining or forwarding raw diagnostics/credentials.
    child.stderr.on('data', () => {});
    child.on('error', () => this.disconnect());
    child.on('exit', () => this.disconnect());
    const grant = await new Promise<Grant>((resolve, reject) => {
      const timeout = setTimeout(() => this.disconnect(), IO_DEADLINE);
      this.startupReject = () => {
        clearTimeout(timeout);
        reject(new BoundaryError());
      };
      let received = false;
      this.grantReader = new FrameReader(
        child.stdout,
        (value) => {
          if (received) {
            this.disconnect();
            return;
          }
          received = true;
          try {
            const grant = grantValue(value, config);
            clearTimeout(timeout);
            this.startupReject = undefined;
            resolve(grant);
          } catch {
            this.disconnect();
          }
        },
        () => this.disconnect(),
      );
      void writeFrame(child.stdin, {
        schema_version: 'avencrew.local-launch/1',
        workspace_id: config.workspaceId,
        actor_id: config.actorId,
        main_pid: process.pid,
        client_build_digest: config.buildDigest,
      }).then(
        () => child.stdin.end(),
        () => this.disconnect(),
      );
    });
    if (this.failed) throw new BoundaryError();
    const socket = connect(grant.socket_path);
    this.socket = socket;
    const challenge = await new Promise<string>((resolve, reject) => {
      const timeout = setTimeout(() => this.disconnect(), IO_DEADLINE);
      this.startupReject = () => {
        clearTimeout(timeout);
        reject(new BoundaryError());
      };
      this.reader = new FrameReader(
        socket,
        (value) => {
          if (typeof value !== 'string' || !HEX.test(value)) {
            this.disconnect();
            return;
          }
          clearTimeout(timeout);
          this.startupReject = undefined;
          this.reader?.close();
          this.reader = new FrameReader(
            socket,
            (value) => this.reply(value),
            () => this.disconnect(),
          );
          resolve(value);
        },
        () => this.disconnect(),
      );
    });
    const handshake = request('handshake', {
      supported_versions: ['1.0'],
      client_kind: 'electron_main',
      build_digest: config.buildDigest,
      nonce: randomBytes(32).toString('hex'),
      challenge_response: '',
      requested_workspace_id: config.workspaceId,
    });
    const proof = createHmac('sha256', Buffer.from(grant.session_key, 'hex'))
      .update(
        canonical({
          domain: 'avencrew.local-handshake/1',
          challenge: grant.challenge,
          connection_challenge: challenge,
          generation: grant.generation,
          scope_id: grant.scope_id,
          peer_uid: process.geteuid?.(),
          peer_pid: process.pid,
          request: handshake.payload,
        }),
      )
      .digest('hex');
    (handshake.payload.body as Record<string, unknown>).challenge_response = proof;
    const result = (await this.send(handshake, IO_DEADLINE)) as Record<string, unknown>;
    if (
      result.selected_version !== '1.0' ||
      result.supervisor_generation !== grant.generation ||
      result.authenticated_scope_id !== grant.scope_id ||
      result.schema_min !== 'sqlite:L0-L2' ||
      result.schema_max !== 'sqlite:L0-L2' ||
      result.max_frame_bytes !== 1048576
    )
      throw new BoundaryError();
    // The grant is scoped to this authenticated connection; never retain its key.
    grant.session_key = '';
    grant.challenge = '';
    if (this.failed) throw new BoundaryError();
    this.manifest = result.capability_manifest_digest as string;
    this.ready = true;
  }
  private reply(value: unknown): void {
    const parsed = parseWireMessage(JSON.stringify(value));
    if (!parsed.ok || (value as { message_kind?: unknown }).message_kind !== 'rpc_response') {
      this.disconnect();
      return;
    }
    const reply = (value as { payload: Reply }).payload;
    const pending = this.pending.get(reply.request_id);
    if (!pending || pending.operation !== reply.operation) {
      this.disconnect();
      return;
    }
    clearTimeout(pending.timer);
    this.pending.delete(reply.request_id);
    if (!reply.ok) pending.reject(new BoundaryError());
    else pending.resolve(reply.result);
  }
  private async send(message: ReturnType<typeof request>, deadline = 10000): Promise<unknown> {
    if (
      this.failed ||
      !this.socket ||
      this.pending.size >= 16 ||
      !validateMessage(message, { surface: 'client' }).ok
    )
      throw new BoundaryError();
    const socket = this.socket;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => this.disconnect(), deadline);
      this.pending.set(message.payload.request_id, {
        operation: message.payload.operation,
        resolve,
        reject,
        timer,
      });
      void writeFrame(socket, message).catch(() => this.disconnect());
    });
  }
  async describeCapabilities(): Promise<CapabilityDescription> {
    if (!this.ready) throw new BoundaryError();
    const capabilities = (await this.send(
      request('describe_capabilities', {}),
    )) as CapabilityDescription;
    if (capabilities.capability_manifest_digest !== this.manifest) {
      this.disconnect();
      throw new BoundaryError();
    }
    return capabilities;
  }
  async command(body: RpcRequestSubmitCommandBody): Promise<CommandReceipt> {
    if (!this.ready) throw new BoundaryError();
    const receipt = await this.send(request('submit_command', body));
    if (!validateCommandReceiptPayload(receipt).ok) {
      this.disconnect();
      throw new BoundaryError();
    }
    return receipt as CommandReceipt;
  }
  disconnect(): void {
    if (this.failed) return;
    this.failed = true;
    this.ready = false;
    this.startupReject?.();
    this.startupReject = undefined;
    this.reader?.close();
    this.reader = undefined;
    this.grantReader?.close();
    this.grantReader = undefined;
    this.socket?.destroy();
    this.socket = undefined;
    this.child?.stdin.destroy();
    this.child?.stdout.destroy();
    this.child?.stderr.destroy();
    for (const entry of this.pending.values()) {
      clearTimeout(entry.timer);
      entry.reject(new BoundaryError());
    }
    this.pending.clear();
  }
}
