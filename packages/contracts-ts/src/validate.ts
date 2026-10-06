/**
 * Client-side wire validation. Rust remains the canonical contract source.
 * The build-generated validator handles every structural definition and nested
 * portable scalar/consistency rule; these wrappers add surface and JSON ingress
 * checks. Validation authenticates nobody and grants no authority.
 */
import { validateWire } from './wire-validator.mjs';

/** Canonical decimal counter: `0`, or `[1-9][0-9]*`, parseable as int64. */
const COUNTER = /^(0|[1-9][0-9]{0,18})$/;
/** Counter that must be greater than zero. */
const POSITIVE_COUNTER = /^[1-9][0-9]{0,18}$/;
/** Exact money spelling: signed, no leading zeros, exactly six fraction digits. */
const MONEY = /^-?(0|[1-9][0-9]*)\.[0-9]{6}$/;

/** Largest int64, matching Rust's `i64::MAX`. */
const I64_MAX = 9223372036854775807n;

/** One validation failure, with the rule that produced it. */
export interface WireViolation {
  /** Dotted path to the offending value, e.g. `payload.epoch`. */
  readonly path: string;
  /** Machine-stable reason code. */
  readonly code: string;
}

export type ValidationResult =
  | { readonly ok: true }
  | { readonly ok: false; readonly violations: readonly WireViolation[] };

function ok(): ValidationResult {
  return { ok: true };
}

function fail(violations: readonly WireViolation[]): ValidationResult {
  return { ok: false, violations };
}

/** Assert `value` is a string matching `pattern`. */
function needString(
  value: unknown,
  path: string,
  pattern: RegExp,
  code: string,
  violations: WireViolation[],
): string | undefined {
  if (typeof value !== 'string') {
    violations.push({ path, code: `${code}-not-a-string` });
    return undefined;
  }
  if (pattern.exec(value)?.[0] !== value) {
    violations.push({ path, code });
    return undefined;
  }
  return value;
}

/** Assert `value` is a decimal-string counter within int64. */
function needCounter(
  value: unknown,
  path: string,
  positive: boolean,
  violations: WireViolation[],
): bigint | undefined {
  const code = positive ? 'invalid-positive-counter' : 'invalid-counter';
  const text = needString(value, path, positive ? POSITIVE_COUNTER : COUNTER, code, violations);
  if (text === undefined) return undefined;
  const parsed = BigInt(text);
  if (parsed > I64_MAX) {
    violations.push({ path, code: 'counter-out-of-int64-range' });
    return undefined;
  }
  return parsed;
}

/**
 * Assert a real UTC calendar instant with exactly six fractional digits.
 *
 * Deliberately stricter than `Date.parse`: `2026-02-30` and a leap second such
 * as `23:59:60` are rejected, matching chrono round-trip in
 * `crates/contracts/src/scalars.rs`.
 */
function needInstant(value: unknown, path: string, violations: WireViolation[]): string | undefined {
  const SHAPE = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{6}Z$/;
  const text = needString(value, path, SHAPE, 'invalid-instant', violations);
  if (text === undefined) return undefined;

  const datePart = text.slice(0, 10);
  const timePart = text.slice(11, 26);
  // The SHAPE regex already fixed the digit counts, but strict index access
  // still requires an explicit guard rather than a non-null assertion.
  const dateParts = datePart.split('-');
  const timeParts = timePart.replace('Z', '').split(':');
  if (dateParts.length !== 3 || timeParts.length !== 3) {
    violations.push({ path, code: 'invalid-instant-shape' });
    return undefined;
  }
  const year = Number(dateParts[0]);
  const month = Number(dateParts[1]);
  const day = Number(dateParts[2]);
  const hour = Number(timeParts[0]);
  const minute = Number(timeParts[1]);
  const second = Number((timeParts[2] ?? '').slice(0, 2));

  const leapYear = (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
  const daysInMonth = [31, leapYear ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  const monthLength = daysInMonth[month - 1] ?? 0;
  if (month < 1 || month > 12 || day < 1 || day > monthLength) {
    violations.push({ path, code: 'invalid-instant-calendar-date' });
    return undefined;
  }
  // Second 60 is a leap second; the journal stores UTC microseconds, never one.
  if (hour > 23 || minute > 59 || second > 59) {
    violations.push({ path, code: 'invalid-instant-time' });
    return undefined;
  }
  return text;
}

const CLIENT_OPERATIONS = new Set([
  'handshake', 'describe_capabilities', 'submit_task', 'submit_command',
  'get_task_snapshot', 'subscribe', 'answer_clarification', 'review_artifact',
  'decide_action', 'get_artifact', 'prepare_transfer', 'query_transfer',
  'request_transfer_abort', 'get_usage',
]);

export interface ValidateOptions {
  /** The caller must authenticate the peer independently of this selection. */
  readonly surface?: 'client' | 'internal';
}

/**
 * Validate a JSON-decoded object. Duplicate keys and original number spellings
 * cannot be recovered here; use parseWireMessage for untrusted JSON text.
 * No fields are coerced, defaulted or removed by the schema validator.
 */
export function validateMessage(input: unknown, options: ValidateOptions = {}): ValidationResult {
  if (!validateWire(input)) return fail([{ path: '', code: 'invalid-wire-shape' }]);
  const envelope = input as { message_kind: string; payload: Record<string, unknown> };
  if (options.surface === 'client' && (envelope.message_kind !== 'rpc_request'
    || typeof envelope.payload.operation !== 'string'
    || !CLIENT_OPERATIONS.has(envelope.payload.operation))) {
    return fail([{ path: 'message_kind', code: 'operation-not-on-client-surface' }]);
  }
  // The model's normalized tool arguments have an additional UTF-8 byte limit.
  if (envelope.message_kind === 'normalized_model_response') {
    const items = envelope.payload.items as Array<Record<string, unknown>>;
    for (const item of items) {
      if (item.kind === 'tool_proposal'
        && new TextEncoder().encode(JSON.stringify(item.arguments)).byteLength > 262144) {
        return fail([{ path: 'payload.items', code: 'tool-argument-byte-limit' }]);
      }
    }
  }
  return ok();
}

/** Single JSON ingress path: bounded parsing, duplicates, then full validation. */
export function parseWireMessage(text: string, options: ValidateOptions = {}): ValidationResult & { readonly value?: unknown } {
  const parsed = parseStrictJson(text);
  if (!parsed.ok) return parsed;
  const checked = validateMessage(parsed.value, options);
  return checked.ok ? { ok: true, value: parsed.value } : checked;
}

/**
 * Parse JSON text while rejecting repeated object keys at any depth.
 *
 * `JSON.parse` silently keeps the last duplicate, so it cannot be used for a
 * wire input. This walks the raw text and reports the first repeat, matching
 * `crates/contracts/src/json.rs`. Escaped spellings of the same key (`"x"` and
 * `"x"`) count as a repeat, as Rust does.
 *
 * Returns the parsed value or a violation.
 */
export function parseStrictJson(text: string): ValidationResult & { readonly value?: unknown } {
  if (text.length > 1048576 || new TextEncoder().encode(text).byteLength > 1048576) {
    return fail([{ path: '', code: 'frame-byte-limit' }]);
  }
  try {
    const value = scanForDuplicateKeys(text);
    return { ok: true, value };
  } catch (error) {
    const code = error instanceof JsonIngressError ? error.message : 'malformed-json';
    return fail([{ path: '', code }]);
  }
}

/**
 * Scan raw JSON text for a repeated key, then delegate to `JSON.parse`.
 *
 * A proper tokeniser is overkill here: this only needs to find object keys, and
 * JSON keys are the only string literals that are followed by a colon in an
 * object context. Throws on duplicate keys and invalid Unicode.
 */
class JsonIngressError extends Error {}

function scanForDuplicateKeys(text: string): unknown {
  const stack: Set<string>[] = [];
  let index = 0;
  let inString = false;
  let escaped = false;
  let stringStart = 0;
  let depth = 0;

  while (index < text.length) {
    const char = text[index] as string;
    if (inString) {
      if (escaped) escaped = false;
      else if (char === '\\') escaped = true;
      else if (char === '"') {
        inString = false;
        const raw = text.slice(stringStart, index + 1);
        const decoded = JSON.parse(raw) as string;
        if (/[\uD800-\uDFFF]/u.test(decoded)) throw new JsonIngressError('invalid-unicode');
        const after = skipWhitespace(text, index + 1);
        if (text[after] === ':') {
          const key = decoded;
          const current = stack[stack.length - 1];
          if (current !== undefined && current.has(key)) {
            throw new JsonIngressError('duplicate-key');
          }
          current?.add(key);
        }
      }
    } else if (char === '"') {
      inString = true;
      escaped = false;
      stringStart = index;
    } else if (char === '{') {
      if (++depth >= 128) throw new JsonIngressError('json-depth-limit');
      stack.push(new Set<string>());
    } else if (char === '}') {
      depth -= 1;
      stack.pop();
    } else if (char === '-' || /[0-9]/.test(char)) {
      const token = /^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/.exec(text.slice(index))?.[0];
      if (token !== undefined) {
        if (token === '-0' || (/[.eE]/.test(token) && Number.isInteger(Number(token)))) {
          throw new JsonIngressError('ambiguous-json-integer');
        }
        index += token.length - 1;
      }
    } else if (char === '[') {
      if (++depth >= 128) throw new JsonIngressError('json-depth-limit');
    } else if (char === ']') {
      depth -= 1;
    }
    index += 1;
  }
  const value: unknown = JSON.parse(text);
  // JavaScript cannot preserve every integer supported by serde_json. Refuse
  // unsafe numeric values rather than silently alter opaque tool data. Domain
  // counters and money are strings and retain their full specified precision.
  const pending: unknown[] = [value];
  while (pending.length > 0) {
    const item = pending.pop();
    if (typeof item === 'number' && (!Number.isFinite(item)
      || (Number.isInteger(item) && !Number.isSafeInteger(item)))) {
      throw new JsonIngressError('unsafe-json-number');
    }
    if (Array.isArray(item)) { for (const child of item) pending.push(child); }
    else if (item !== null && typeof item === 'object') {
      for (const child of Object.values(item)) pending.push(child);
    }
  }
  return value;
}

function skipWhitespace(text: string, from: number): number {
  let at = from;
  while (at < text.length && ' \t\n\r'.includes(text[at] as string)) at += 1;
  return at;
}

/** Validate a decoded `CommandReceipt`, for consumers that handle receipts alone. */
export function validateCommandReceiptPayload(input: unknown): ValidationResult {
  return validateMessage({ schema_version: '1.0', message_kind: 'command_receipt', payload: input });
}

/** Validate a counter string on its own. Exposed for scalar-level tests. */
export function validateCounter(value: unknown, positive = false): ValidationResult {
  const violations: WireViolation[] = [];
  needCounter(value, 'counter', positive, violations);
  return violations.length === 0 ? ok() : fail(violations);
}

/** Validate an instant string on its own. Exposed for scalar-level tests. */
export function validateInstant(value: unknown): ValidationResult {
  const violations: WireViolation[] = [];
  needInstant(value, 'instant', violations);
  return violations.length === 0 ? ok() : fail(violations);
}

/** Validate a money string on its own. Exposed for scalar-level tests. */
export function validateMoney(value: unknown): ValidationResult {
  const violations: WireViolation[] = [];
  needString(value, 'money', MONEY, 'invalid-money', violations);
  return violations.length === 0 ? ok() : fail(violations);
}