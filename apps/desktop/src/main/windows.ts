import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { BrowserWindow, type Session, session } from 'electron';

export const DOCUMENT_URL = new URL('../renderer/index.html', import.meta.url).href;
const PRELOAD = fileURLToPath(new URL('../preload/preload.cjs', import.meta.url));
function isolatedSession(allowed: (url: string) => boolean): Session {
  const isolated = session.fromPartition(`avencrew-${randomUUID()}`, { cache: false });
  isolated.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
  isolated.setPermissionCheckHandler(() => false);
  isolated.on('will-download', (event) => event.preventDefault());
  isolated.webRequest.onBeforeRequest((details, callback) =>
    callback({ cancel: !allowed(details.url) }),
  );
  return isolated;
}
function lockWindow(window: BrowserWindow): void {
  const contents = window.webContents;
  contents.on('will-navigate', (event) => event.preventDefault());
  contents.on('will-frame-navigate', (event) => event.preventDefault());
  contents.on('will-redirect', (event) => event.preventDefault());
  contents.on('will-attach-webview', (event) => event.preventDefault());
  contents.setWindowOpenHandler(() => ({ action: 'deny' }));
}
export function createTrustedWindow(visible = true): BrowserWindow {
  const window = new BrowserWindow({
    width: 1080,
    height: 720,
    title: 'Avencrew',
    backgroundColor: '#12131a',
    show: false,
    webPreferences: {
      preload: PRELOAD,
      session: isolatedSession((url) => {
        try {
          const parsed = new URL(url);
          const root = new URL('../renderer/', import.meta.url);
          return (
            parsed.protocol === 'file:' &&
            parsed.href.startsWith(root.href) &&
            !parsed.search &&
            !parsed.hash
          );
        } catch {
          return false;
        }
      }),
      nodeIntegration: false,
      nodeIntegrationInWorker: false,
      nodeIntegrationInSubFrames: false,
      contextIsolation: true,
      sandbox: true,
      webviewTag: false,
      webSecurity: true,
    },
  });
  lockWindow(window);
  if (visible) window.once('ready-to-show', () => window.show());
  return window;
}
/** Policy only: artifact retrieval and audience decisions belong to a later consumer. */
export function createArtifactPreview(): BrowserWindow {
  const window = new BrowserWindow({
    show: false,
    webPreferences: {
      session: isolatedSession((url) => url.startsWith('data:text/html,')),
      javascript: false,
      nodeIntegration: false,
      nodeIntegrationInWorker: false,
      nodeIntegrationInSubFrames: false,
      contextIsolation: true,
      sandbox: true,
      webviewTag: false,
      webSecurity: true,
    },
  });
  lockWindow(window);
  return window;
}
