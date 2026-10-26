import type {
  CapabilityDescription,
  CommandReceipt,
  RpcRequestSubmitCommandBody,
} from '@avencrew/contracts-ts/wire';

export type ControlKind = 'steer' | 'pause' | 'stop';
export type ControlInput<K extends ControlKind> = Omit<
  Extract<RpcRequestSubmitCommandBody, { kind: K }>,
  'kind'
>;
export type BridgeFailure =
  | 'DISCONNECTED'
  | 'INVALID_INPUT'
  | 'DENIED'
  | 'UNAVAILABLE'
  | 'REJECTED';
export type BridgeResult<T> = { ok: true; value: T } | { ok: false; code: BridgeFailure };
export interface DesktopBridge {
  describeCapabilities(): Promise<BridgeResult<CapabilityDescription>>;
  steer(input: ControlInput<'steer'>): Promise<BridgeResult<CommandReceipt>>;
  pause(input: ControlInput<'pause'>): Promise<BridgeResult<CommandReceipt>>;
  stop(input: ControlInput<'stop'>): Promise<BridgeResult<CommandReceipt>>;
}
export const CHANNELS = Object.freeze({
  describeCapabilities: 'avencrew:describe-capabilities',
  steer: 'avencrew:steer',
  pause: 'avencrew:pause',
  stop: 'avencrew:stop',
});
