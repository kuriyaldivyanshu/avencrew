import assert from 'node:assert/strict';
import { symlink, writeFile } from 'node:fs/promises';
import { once } from 'node:events';
import type { ChildProcess } from 'node:child_process';
import { app, BrowserWindow, ipcMain } from 'electron';
import { composeDesktop, ownsInvocation } from '../src/main/composition.js';
import { SupervisorClient } from '../src/main/client.js';
import { createArtifactPreview, DOCUMENT_URL } from '../src/main/windows.js';
import { CHANNELS, PRELOAD_AUTHORIZE } from '../src/shared/bridge.js';

// This is a separate synthetic main entry, never an app mode or renderer API.
const root = process.argv[2];
const executable = process.argv[3];
assert(root && executable);
app.setPath('userData', `${root}/electron-profile`);
app.on('window-all-closed', () => {});
const id = (n: number) => `01900000-0000-7000-8000-${n.toString(16).padStart(12, '0')}`;
let owned: ChildProcess | undefined;
const watchdog = setTimeout(() => {
  console.error('native watchdog failed');
  owned?.kill('SIGKILL');
  app.exit(1);
}, 60000);
async function runNative() {
  assert(root && executable);
  console.log(
    JSON.stringify({
      check: 'desktop-native',
      versions: process.versions,
      executable: process.execPath,
      platform: process.platform,
    }),
  );
  try {
    const config = {
      executable,
      dataRoot: root,
      workspaceId: id(1),
      actorId: id(2),
      buildDigest: 'a'.repeat(64),
    };
    const assertHandlersFree = (): void => {
      for (const channel of Object.values(CHANNELS)) {
        ipcMain.handle(channel, () => null);
        ipcMain.removeHandler(channel);
      }
      assert.equal(ipcMain.listenerCount(PRELOAD_AUTHORIZE), 0);
      assert.equal(BrowserWindow.getAllWindows().length, 0);
    };
    // A partial installation must not remove a pre-existing owner's handler.
    ipcMain.handle(CHANNELS.pause, () => 'existing-owner');
    await assert.rejects(composeDesktop(undefined, false), /Desktop unavailable/);
    assert.throws(() => ipcMain.handle(CHANNELS.pause, () => null));
    ipcMain.removeHandler(CHANNELS.pause);
    assertHandlersFree();
    const originalLoad = BrowserWindow.prototype.loadURL;
    BrowserWindow.prototype.loadURL = async () => {
      throw new Error('synthetic local document failure');
    };
    try {
      await assert.rejects(composeDesktop(undefined, false), /Desktop unavailable/);
    } finally {
      BrowserWindow.prototype.loadURL = originalLoad;
    }
    assertHandlersFree();
    // Real bootstrap failure after grant/challenge acquisition must release its
    // owned child and store lock before a subsequent launch is attempted.
    type Internals = {
      child: ChildProcess;
      send(this: Internals, ...args: unknown[]): Promise<unknown>;
    };
    const internals = SupervisorClient.prototype as unknown as Internals;
    const originalSend = internals.send;
    for (const closeWindow of [false, true]) {
      internals.send = async function (...args: unknown[]) {
        owned = this.child;
        if (closeWindow) {
          for (const candidate of BrowserWindow.getAllWindows()) candidate.destroy();
          return originalSend.apply(this, args);
        }
        throw new Error('synthetic preauthentication failure');
      };
      try {
        if (closeWindow) await assert.rejects(composeDesktop(config, false), /Desktop unavailable/);
        else await assert.rejects(SupervisorClient.launch(config), /Supervisor unavailable/);
      } finally {
        internals.send = originalSend;
      }
      assert(owned);
      assert(owned.exitCode !== null || owned.signalCode !== null);
      console.log(
        `native failed bootstrap child reaped before relaunch; close-during-launch=${closeWindow}`,
      );
      owned = undefined;
      assertHandlersFree();
    }
    // Authenticated lifetime remains independent even if the final load fails.
    const originalLaunch = SupervisorClient.launch;
    SupervisorClient.launch = async (...args: Parameters<typeof originalLaunch>) => {
      const result = await originalLaunch(...args);
      owned = (result as unknown as { child: ChildProcess }).child;
      return result;
    };
    BrowserWindow.prototype.loadURL = async () => {
      throw new Error('synthetic late load failure');
    };
    try {
      await assert.rejects(composeDesktop(config, false), /Desktop unavailable/);
    } finally {
      SupervisorClient.launch = originalLaunch;
      BrowserWindow.prototype.loadURL = originalLoad;
    }
    assert(owned?.pid);
    process.kill(owned.pid, 0);
    assertHandlersFree();
    const lateExited = once(owned, 'exit');
    owned.kill('SIGKILL');
    await lateExited;
    owned = undefined;
    console.log(
      'native composition failure cleanup preserves existing handlers and authenticated lifetime',
    );
    await symlink(root, `${root}/root-alias`);
    const disconnected = await composeDesktop(undefined, false);
    assert.deepEqual(
      await disconnected.window.webContents.executeJavaScript(
        'window.avencrew.describeCapabilities()',
      ),
      { ok: false, code: 'DISCONNECTED' },
    );
    disconnected.window.destroy();
    const { window, client } = await composeDesktop(
      {
        executable,
        dataRoot: `${root}/root-alias`,
        workspaceId: id(1),
        actorId: id(2),
        buildDigest: 'a'.repeat(64),
      },
      false,
    );
    assert(client);
    owned = (client as unknown as { child: ChildProcess }).child;
    assert(owned.pid);
    const contents = window.webContents;
    const probe = await contents.executeJavaScript(
      `({keys:Object.keys(window.avencrew).sort(), frozen:Object.isFrozen(window.avencrew), node:typeof process, require:typeof require, ipc:typeof ipcRenderer, secret:typeof window.session_key})`,
    );
    assert.deepEqual(probe, {
      keys: ['describeCapabilities', 'pause', 'steer', 'stop'],
      frozen: true,
      node: 'undefined',
      require: 'undefined',
      ipc: 'undefined',
      secret: 'undefined',
    });

    assert.equal(await contents.executeJavaScript('Notification.requestPermission()'), 'denied');
    const capabilities = await contents.executeJavaScript('window.avencrew.describeCapabilities()');
    assert.equal(capabilities.ok, true);
    const steer = {
      command_id: id(400),
      run_id: id(300),
      idempotency_key: 'desktop-steer',
      payload: { text: 'Retain this exact steering', expected_revision: '1' },
    };
    const invoke = (method: string, input: unknown) =>
      contents.executeJavaScript(`window.avencrew.${method}(${JSON.stringify(input)})`);
    const original = await invoke('steer', steer);
    assert.equal(original.ok, true);
    assert.equal(original.value.kind, 'accepted');
    assert.deepEqual(await invoke('steer', steer), original);
    assert.deepEqual(
      await invoke('steer', { ...steer, payload: { ...steer.payload, text: 'changed' } }),
      { ok: false, code: 'REJECTED' },
    );
    for (const input of [
      { ...steer, kind: 'stop' },
      { ...steer, extra: 1 },
      { ...steer, payload: { ...steer.payload, expected_revision: 1 } },
      { operation: 'harness.start' },
      null,
    ]) {
      assert.deepEqual(await invoke('steer', input), { ok: false, code: 'INVALID_INPUT' });
    }
    const pause = await invoke('pause', {
      command_id: id(410),
      run_id: id(300),
      idempotency_key: 'desktop-pause',
      payload: { reason: 'Pause requested', expected_revision: original.value.row_version },
    });
    assert.equal(pause.ok, true);
    assert.equal(pause.value.kind, 'accepted');
    const stop = await invoke('stop', {
      command_id: id(420),
      run_id: id(300),
      idempotency_key: 'desktop-stop',
      payload: { reason: 'Stop requested' },
    });
    assert.equal(stop.ok, true);
    assert.equal(stop.value.kind, 'accepted');
    await writeFile(
      `${root}/receipts.json`,
      JSON.stringify([original.value, pause.value, stop.value]),
    );
    console.log(
      'native IPC -> authenticated supervisor: steer/pause/stop accepted; exact retry and changed-byte rejection',
    );
    // Register a test-only preload on the existing isolated session, without a
    // production configuration hook. It lets adversarial frames emit raw IPC.
    const probeId = contents.session.registerPreloadScript({
      type: 'frame',
      filePath: new URL('../preload/hostile.cjs', import.meta.url).pathname,
    });
    await window.loadURL(DOCUMENT_URL);
    console.log('native session test preload loaded');
    assert.deepEqual(await contents.executeJavaScript('window.boundaryProbe'), {
      sandboxed: true,
      contextIsolated: true,
      isMainFrame: true,
    });
    assert.deepEqual(
      await contents.executeJavaScript(
        `window.rawInvoke(${JSON.stringify(CHANNELS.describeCapabilities)}, {})`,
      ),
      { ok: false, code: 'INVALID_INPUT' },
    );
    assert.deepEqual(
      await contents.executeJavaScript(
        `window.rawInvoke(${JSON.stringify(CHANNELS.steer)}, {operation:'harness.start'})`,
      ),
      { ok: false, code: 'INVALID_INPUT' },
    );
    // Test instrumentation bypasses only CSP to create an otherwise forbidden
    // subframe; production still supplies its unchanged navigation/IPC policy.
    console.log('native raw schema probes passed');
    contents.debugger.attach();
    await contents.debugger.sendCommand('Page.setBypassCSP', { enabled: true });
    await contents.executeJavaScript(
      `new Promise(resolve => {const frame=document.createElement('iframe');frame.onload=()=>resolve(true);frame.src='about:blank';document.body.append(frame);})`,
    );
    console.log('native adversarial subframe created');
    const subframe = contents.mainFrame.frames[0];
    assert(subframe);
    assert.equal(await subframe.executeJavaScript('typeof window.avencrew'), 'undefined');
    assert.equal(await subframe.executeJavaScript('typeof window.rawInvoke'), 'undefined');
    assert.equal(
      ownsInvocation(window, { sender: contents, senderFrame: subframe } as never),
      false,
    );
    await contents.executeJavaScript("document.querySelector('iframe').remove()");
    await contents.debugger.sendCommand('Page.setBypassCSP', { enabled: false });
    contents.debugger.detach();
    contents.session.unregisterPreloadScript(probeId);
    console.log(
      'native owning-window raw schema/internal denied; production subframe bridge absent and live frame guard denied',
    );
    // A hostile test preload can send raw IPC, but belongs to another WebContents.
    const wrong = new BrowserWindow({
      show: false,
      webPreferences: {
        preload: new URL('../preload/hostile.cjs', import.meta.url).pathname,
        sandbox: true,
        contextIsolation: true,
        nodeIntegration: false,
        nodeIntegrationInSubFrames: true,
      },
    });
    await wrong.loadURL(DOCUMENT_URL);
    assert.deepEqual(
      await wrong.webContents.executeJavaScript(
        `window.rawInvoke(${JSON.stringify(CHANNELS.describeCapabilities)})`,
      ),
      { ok: false, code: 'DENIED' },
    );
    assert.deepEqual(
      await wrong.webContents.executeJavaScript(
        `window.rawInvoke(${JSON.stringify(CHANNELS.steer)},${JSON.stringify(steer)})`,
      ),
      { ok: false, code: 'DENIED' },
    );
    console.log('native wrong-window raw IPC probes passed');
    wrong.webContents.debugger.attach();
    await wrong.webContents.debugger.sendCommand('Page.setBypassCSP', { enabled: true });
    await wrong.webContents.executeJavaScript(
      `new Promise(resolve=>{const frame=document.createElement('iframe');frame.onload=()=>resolve(true);frame.src=${JSON.stringify(DOCUMENT_URL)};document.body.append(frame);})`,
    );
    const hostileFrame = wrong.webContents.mainFrame.frames[0];
    assert(hostileFrame);
    assert.deepEqual(
      await hostileFrame.executeJavaScript(
        `window.rawInvoke(${JSON.stringify(CHANNELS.describeCapabilities)})`,
      ),
      { ok: false, code: 'DENIED' },
    );
    wrong.webContents.debugger.detach();
    wrong.destroy();
    console.log('native wrong-window raw IPC denied');
    // Programmatic main navigation bypasses will-navigate intentionally; the
    // production preload must still refuse its API on the wrong document.
    const wrongDocument = new BrowserWindow({
      show: false,
      webPreferences: {
        preload: new URL('../preload/preload.cjs', import.meta.url).pathname,
        sandbox: true,
        contextIsolation: true,
        nodeIntegration: false,
      },
    });
    await wrongDocument.loadURL('data:text/html,<h1>hostile</h1>');
    assert.equal(
      await wrongDocument.webContents.executeJavaScript('typeof window.avencrew'),
      'undefined',
    );
    wrongDocument.destroy();
    console.log('native wrong-document preload absent');
    assert.equal(
      await contents.executeJavaScript(
        `(()=>{const view=document.createElement('webview');view.src='https://example.invalid/';document.body.append(view);return typeof view.getWebContentsId;})()`,
      ),
      'undefined',
    );
    const artifact = createArtifactPreview();
    await artifact.loadURL(
      'data:text/html,' +
        encodeURIComponent(
          `<script>globalThis.hostileRan=true;fetch('https://example.invalid/');</script><h1>Hostile artifact</h1><iframe src="${DOCUMENT_URL}"></iframe>`,
        ),
    );

    artifact.webContents.debugger.attach();
    const artifactProbe = await artifact.webContents.debugger.sendCommand('Runtime.evaluate', {
      expression:
        'JSON.stringify({bridge:typeof window.avencrew,script:typeof hostileRan,node:typeof process,require:typeof require})',
      returnByValue: true,
    });
    assert.deepEqual(JSON.parse(artifactProbe.result.value), {
      bridge: 'undefined',
      script: 'undefined',
      node: 'undefined',
      require: 'undefined',
    });
    const deniedFetch = await artifact.webContents.debugger.sendCommand('Runtime.evaluate', {
      expression: "fetch('https://example.invalid/').then(()=> 'allowed',()=> 'denied')",
      awaitPromise: true,
      returnByValue: true,
    });
    assert.equal(deniedFetch.result.value, 'denied');
    artifact.webContents.debugger.detach();
    assert.equal(artifact.webContents.session.isPersistent(), false);
    assert.notEqual(artifact.webContents.session, contents.session);
    const downloadDenied = new Promise<boolean>((resolve, reject) => {
      const deadline = setTimeout(
        () => reject(new Error('download policy probe did not complete')),
        5000,
      );
      artifact.webContents.session.once('will-download', (event) => {
        clearTimeout(deadline);
        resolve(event.defaultPrevented);
      });
    });
    artifact.webContents.downloadURL('data:text/html,%3Ch1%3Eowned-download-probe%3C/h1%3E');
    assert.equal(await downloadDenied, true);
    console.log('native production webview API absent and actual download canceled');
    artifact.destroy();
    const originalUrl = contents.getURL();
    await contents.executeJavaScript(
      "window.open('https://example.invalid/'); location.href='https://example.invalid/';",
    );
    assert.equal(contents.getURL(), originalUrl);
    await contents.executeJavaScript("history.replaceState(null,'','#hostile')");
    assert.deepEqual(await contents.executeJavaScript('window.avencrew.describeCapabilities()'), {
      ok: false,
      code: 'DENIED',
    });
    console.log(
      'native live owning-main wrong-document IPC denied; permission and artifact network denied',
    );
    window.destroy();
    process.kill(owned.pid, 0);
    console.log(
      'native hostile sender/schema/document/artifact/navigation checks; supervisor survives window/client close',
    );
    // Explicit fixture teardown, after proving production close did not stop it.
    const exited = once(owned, 'exit');
    owned.kill('SIGKILL');
    await exited;
    owned = undefined;
  } finally {
    owned?.kill('SIGKILL');
    clearTimeout(watchdog);
    for (const window of BrowserWindow.getAllWindows()) window.destroy();
  }
  app.exit(0);
}
app
  .whenReady()
  .then(runNative)
  .catch((error) => {
    console.error('native test failure:', String(error));
    owned?.kill('SIGKILL');
    app.exit(1);
  });
