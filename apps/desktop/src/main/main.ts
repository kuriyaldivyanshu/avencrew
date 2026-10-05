/**
 * Electron main process.
 *
 * Responsibilities in this scaffold: create one window, load only local content,
 * and refuse navigation and window opening. Nothing else.
 *
 * Privilege settings are the point of this file, so they are spelled out rather
 * than inherited from a helper:
 *
 *   nodeIntegration: false  renderer gets no Node
 *   contextIsolation: true  preload and page scripts run in separate worlds
 *   sandbox: true           renderer OS sandboxed
 *
 * A sandboxed renderer is *not* a command sandbox. Running shell or tool
 * activity remains the harness's job under the guest isolation profile; see
 * docs/architecture/technology-baseline-and-tools.md.
 */
import { fileURLToPath } from 'node:url';

import { BrowserWindow, app, shell } from 'electron';

/** Built renderer document. Fixed at build time; never user or network input. */
const RENDERER_HTML = fileURLToPath(new URL('../renderer/index.html', import.meta.url));

/** Built preload bundle. CommonJS, required by `sandbox: true`. */
const PRELOAD = fileURLToPath(new URL('../preload/preload.cjs', import.meta.url));

function createWindow(): BrowserWindow {
  const window = new BrowserWindow({
    width: 1080,
    height: 720,
    title: 'Avencrew',
    backgroundColor: '#12131a',
    show: false,
    webPreferences: {
      preload: PRELOAD,
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: true,
    },
  });

  window.once('ready-to-show', () => window.show());

  // Refuse in-page navigation and popups. The renderer is a fixed local
  // document; anything trying to navigate it is not this application.
  window.webContents.on('will-navigate', (event) => {
    event.preventDefault();
  });

  window.webContents.setWindowOpenHandler(({ url }) => {
    // External links open in the user's browser, never in an Electron window
    // that would inherit this process's privileges.
    if (url.startsWith('https://')) {
      void shell.openExternal(url);
    }
    return { action: 'deny' };
  });

  window.webContents.on('will-attach-webview', (event) => {
    event.preventDefault();
  });

  void window.loadFile(RENDERER_HTML);
  return window;
}

app.whenReady().then(() => {
  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  // macOS convention: the app stays resident. No background work exists yet, so
  // this does not imply any scheduler, supervisor or agent is running.
  if (process.platform !== 'darwin') {
    app.quit();
  }
});