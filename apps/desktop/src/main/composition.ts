import { parseWireMessage } from '@avencrew/contracts-ts/validate';
import type { RpcRequestSubmitCommandBody } from '@avencrew/contracts-ts/wire';
import {
  type BrowserWindow,
  type IpcMainEvent,
  type IpcMainInvokeEvent,
  ipcMain,
  type WebContents,
} from 'electron';
import {
  type BridgeResult,
  CHANNELS,
  type ControlKind,
  PRELOAD_AUTHORIZE,
} from '../shared/bridge.js';
import { request, SupervisorClient, type TrustedLaunch } from './client.js';
import { RejectedError } from './transport.js';
import { createTrustedWindow, DOCUMENT_URL } from './windows.js';

/** Checked anew on every message, including current frame identity after navigation. */
export function ownsInvocation(
  window: BrowserWindow,
  event: IpcMainInvokeEvent | IpcMainEvent,
): boolean {
  if (window.isDestroyed() || event.sender !== window.webContents || event.sender.isDestroyed())
    return false;
  const frame = event.senderFrame;
  return (
    frame !== null &&
    !frame.detached &&
    frame === window.webContents.mainFrame &&
    frame.url === DOCUMENT_URL &&
    event.sender.getURL() === DOCUMENT_URL
  );
}
// Synchronous boolean authorization completes before the renderer's startup.
const preloadOwners = new Map<WebContents, BrowserWindow>();
const authorizePreload = (event: IpcMainEvent, ...args: unknown[]): void => {
  const owner = preloadOwners.get(event.sender);
  event.returnValue = args.length === 0 && owner !== undefined && ownsInvocation(owner, event);
};
export async function composeDesktop(config?: TrustedLaunch, visible = true) {
  const window = createTrustedWindow(visible);
  const contents = window.webContents;
  let client: SupervisorClient | undefined;
  const handlers = new Set<string>();
  const launchAbort = new AbortController();
  let disposed = false;
  const dispose = (): void => {
    if (disposed) return;
    disposed = true;
    launchAbort.abort();
    for (const channel of handlers) ipcMain.removeHandler(channel);
    handlers.clear();
    preloadOwners.delete(contents);
    if (preloadOwners.size === 0) ipcMain.removeListener(PRELOAD_AUTHORIZE, authorizePreload);
    client?.disconnect();
  };
  window.once('closed', dispose);
  try {
    client = config ? await SupervisorClient.launch(config, launchAbort.signal) : undefined;
    if (disposed || window.isDestroyed()) throw new Error('Desktop unavailable');
    if (preloadOwners.size === 0) ipcMain.on(PRELOAD_AUTHORIZE, authorizePreload);
    preloadOwners.set(contents, window);
    for (const [method, channel] of Object.entries(CHANNELS)) {
      ipcMain.handle(channel, async (event, ...args: unknown[]): Promise<BridgeResult<unknown>> => {
        if (!ownsInvocation(window, event)) return { ok: false, code: 'DENIED' };
        let body: RpcRequestSubmitCommandBody | undefined;
        if (method === 'describeCapabilities') {
          if (args.length !== 0) return { ok: false, code: 'INVALID_INPUT' };
        } else {
          const input = args[0];
          if (
            args.length !== 1 ||
            !input ||
            typeof input !== 'object' ||
            Array.isArray(input) ||
            Object.hasOwn(input, 'kind')
          )
            return { ok: false, code: 'INVALID_INPUT' };
          const candidate = { ...input, kind: method as ControlKind };
          let encoded: string;
          try {
            encoded = JSON.stringify(request('submit_command', candidate));
          } catch {
            return { ok: false, code: 'INVALID_INPUT' };
          }
          if (!parseWireMessage(encoded, { surface: 'client' }).ok)
            return { ok: false, code: 'INVALID_INPUT' };
          body = candidate as RpcRequestSubmitCommandBody;
        }
        if (!client) return { ok: false, code: 'DISCONNECTED' };
        try {
          const value = body ? await client.command(body) : await client.describeCapabilities();
          return { ok: true, value };
        } catch (error) {
          return { ok: false, code: error instanceof RejectedError ? 'REJECTED' : 'UNAVAILABLE' };
        }
      });
      handlers.add(channel);
    }
    await window.loadURL(DOCUMENT_URL);
    if (disposed || window.isDestroyed()) throw new Error('Desktop unavailable');
    return { window, client };
  } catch {
    dispose();
    client?.disconnect();
    if (!window.isDestroyed()) window.destroy();
    throw new Error('Desktop unavailable');
  }
}
