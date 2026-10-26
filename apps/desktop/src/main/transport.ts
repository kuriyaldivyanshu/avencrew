import type { Readable, Writable } from 'node:stream';
import { parseStrictJson } from '@avencrew/contracts-ts/validate';

export const MAX_FRAME = 1048576;
export const IO_DEADLINE = 5000;
export class BoundaryError extends Error {
  constructor() {
    super('Supervisor unavailable');
  }
}
export function frame(value: unknown): Buffer {
  const bytes = Buffer.from(JSON.stringify(value));
  if (!bytes.length || bytes.length > MAX_FRAME) throw new BoundaryError();
  const head = Buffer.alloc(4);
  head.writeUInt32BE(bytes.length);
  return Buffer.concat([head, bytes]);
}
export function writeFrame(stream: Writable, value: unknown): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      stream.destroy();
      reject(new BoundaryError());
    }, IO_DEADLINE);
    try {
      stream.write(frame(value), (error) => {
        clearTimeout(timer);
        if (error) reject(new BoundaryError());
        else resolve();
      });
    } catch {
      clearTimeout(timer);
      reject(new BoundaryError());
    }
  });
}
/** One bounded frame under assembly; no unbounded read queue or replacement decoding. */
export class FrameReader {
  private buffer = Buffer.alloc(0);
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
    // Consume even a coalesced chunk one frame at a time without retaining it.
    let offset = 0;
    try {
      while (offset < chunk.length && !this.closed) {
        if (!this.buffer.length) this.timer = setTimeout(this.fail, IO_DEADLINE);
        const target = this.buffer.length < 4 ? 4 : 4 + this.buffer.readUInt32BE(0);
        const n = Math.min(target - this.buffer.length, chunk.length - offset);
        this.buffer = Buffer.concat([this.buffer, chunk.subarray(offset, offset + n)]);
        offset += n;
        if (this.buffer.length < 4) continue;
        const length = this.buffer.readUInt32BE(0);
        if (!length || length > MAX_FRAME) throw new BoundaryError();
        if (this.buffer.length !== length + 4) continue;
        const text = new TextDecoder('utf-8', { fatal: true }).decode(this.buffer.subarray(4));
        const parsed = parseStrictJson(text);
        if (!parsed.ok) throw new BoundaryError();
        clearTimeout(this.timer);
        this.timer = undefined;
        this.buffer = Buffer.alloc(0);
        this.receive(parsed.value);
      }
    } catch {
      this.fail();
    }
  };
  close(): void {
    this.closed = true;
    clearTimeout(this.timer);
    this.buffer.fill(0);
    this.buffer = Buffer.alloc(0);
    this.stream.off('data', this.data);
    this.stream.off('error', this.fail);
    this.stream.off('end', this.fail);
    this.stream.off('close', this.fail);
    // Late stream errors must never escape teardown.
    this.stream.on('error', () => {});
  }
}
