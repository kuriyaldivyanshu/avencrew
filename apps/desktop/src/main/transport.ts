import type { Readable, Writable } from 'node:stream';
import { parseStrictJson } from '@avencrew/contracts-ts/validate';

export const MAX_FRAME = 1048576;
export const IO_DEADLINE = 5000;
export class BoundaryError extends Error {
  constructor() {
    super('Supervisor unavailable');
  }
}
/** A validated server rejection; contains no server text or diagnostics. */
export class RejectedError extends BoundaryError {}
const ignoreStreamError = (): void => {};
export function frame(value: unknown): Buffer {
  const bytes = Buffer.from(JSON.stringify(value));
  if (!bytes.length || bytes.length > MAX_FRAME) throw new BoundaryError();
  const head = Buffer.alloc(4);
  head.writeUInt32BE(bytes.length);
  return Buffer.concat([head, bytes]);
}
function completeWrite(
  stream: Writable,
  action: (done: (error?: Error | null) => void) => void,
): Promise<void> {
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (error?: Error | null): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (!stream.listeners('error').includes(ignoreStreamError))
        stream.on('error', ignoreStreamError);
      stream.off('error', onError);
      stream.off('close', onClose);
      if (error) reject(new BoundaryError());
      else resolve();
    };
    const onError = (): void => finish(new BoundaryError());
    const onClose = (): void => finish(new BoundaryError());
    const timer = setTimeout(() => {
      finish(new BoundaryError());
      stream.destroy();
    }, IO_DEADLINE);
    stream.on('error', onError);
    stream.on('close', onClose);
    try {
      action(finish);
    } catch {
      finish(new BoundaryError());
    }
  });
}
export function writeFrame(stream: Writable, value: unknown): Promise<void> {
  return completeWrite(stream, (done) => stream.write(frame(value), done));
}
export function endStream(stream: Writable): Promise<void> {
  return completeWrite(stream, (done) => stream.end(done));
}
/** Fixed header and one bounded payload allocation: each input byte is copied once. */
export class FrameReader {
  private header = Buffer.alloc(4);
  private headerUsed = 0;
  private buffer = Buffer.alloc(0);
  private payloadUsed = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private closed = false;
  constructor(
    private readonly stream: Readable,
    private readonly receive: (value: unknown) => void,
    private readonly failure: () => void,
  ) {
    stream.on('data', this.data);
    stream.on('error', this.fail);
    stream.on('end', this.fail);
    stream.on('close', this.fail);
  }
  private fail = (): void => {
    if (this.closed) return;
    this.close();
    this.failure();
  };
  private data = (chunk: Buffer): void => {
    let offset = 0;
    try {
      while (offset < chunk.length && !this.closed) {
        if (this.headerUsed === 0) this.timer = setTimeout(this.fail, IO_DEADLINE);
        if (this.headerUsed < 4) {
          const n = Math.min(4 - this.headerUsed, chunk.length - offset);
          chunk.copy(this.header, this.headerUsed, offset, offset + n);
          this.headerUsed += n;
          offset += n;
          if (this.headerUsed < 4) continue;
          const length = this.header.readUInt32BE(0);
          if (!length || length > MAX_FRAME) throw new BoundaryError();
          this.buffer = Buffer.alloc(length);
        }
        const n = Math.min(this.buffer.length - this.payloadUsed, chunk.length - offset);
        chunk.copy(this.buffer, this.payloadUsed, offset, offset + n);
        this.payloadUsed += n;
        offset += n;
        if (this.payloadUsed !== this.buffer.length) continue;
        const text = new TextDecoder('utf-8', { fatal: true }).decode(this.buffer);
        const parsed = parseStrictJson(text);
        if (!parsed.ok) throw new BoundaryError();
        clearTimeout(this.timer);
        this.timer = undefined;
        this.buffer.fill(0);
        this.buffer = Buffer.alloc(0);
        this.payloadUsed = 0;
        this.headerUsed = 0;
        this.receive(parsed.value);
      }
    } catch {
      this.fail();
    }
  };
  close(): void {
    if (this.closed) return;
    this.closed = true;
    clearTimeout(this.timer);
    this.timer = undefined;
    this.header.fill(0);
    this.headerUsed = 0;
    this.buffer.fill(0);
    this.buffer = Buffer.alloc(0);
    this.payloadUsed = 0;
    this.stream.off('data', this.data);
    this.stream.off('error', this.fail);
    this.stream.off('end', this.fail);
    this.stream.off('close', this.fail);
    // One stable listener per stream, even if multiple readers are retired.
    if (!this.stream.listeners('error').includes(ignoreStreamError))
      this.stream.on('error', ignoreStreamError);
  }
}
