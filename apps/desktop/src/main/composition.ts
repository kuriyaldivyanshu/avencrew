import { parseWireMessage } from '@avencrew/contracts-ts/validate';
import type { RpcRequestSubmitCommandBody } from '@avencrew/contracts-ts/wire';
import { type BrowserWindow, type IpcMainInvokeEvent, ipcMain } from 'electron';
import { type BridgeResult, CHANNELS, type ControlKind } from '../shared/bridge.js';
import { request, SupervisorClient, type TrustedLaunch } from './client.js';
import { createTrustedWindow, DOCUMENT_URL } from './windows.js';

/** Checked anew on every message, including current frame identity after navigation. */
export function ownsInvocation(window: BrowserWindow, event: IpcMainInvokeEvent): boolean {
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
export async function composeDesktop(config?: TrustedLaunch, visible = true) {
  const window = createTrustedWindow(visible);
  let client: SupervisorClient | undefined;
  try {
    client = config ? await SupervisorClient.launch(config) : undefined;
  } catch {
    window.destroy();
    throw new Error('Supervisor unavailable');
  }
  const handlers: string[] = [];
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
      } catch {
        return { ok: false, code: 'UNAVAILABLE' };
      }
    });
    handlers.push(channel);
  }
  // Closing a window unregisters its bridge but sends no control or process signal.
  window.once('closed', () => {
    for (const channel of handlers) ipcMain.removeHandler(channel);
    client?.disconnect();
  });
  await window.loadURL(DOCUMENT_URL);
  return { window, client };
}
