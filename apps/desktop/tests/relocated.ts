import assert from 'node:assert/strict';
import { app } from 'electron';
import { composeDesktop } from '../src/main/composition.js';
import { DOCUMENT_URL } from '../src/main/windows.js';
const root = process.argv[2];
assert(root);
app.setPath('userData', `${root}/relocated-profile`);
app.on('window-all-closed', () => {});
const watchdog = setTimeout(() => app.exit(1), 15000);
void app
  .whenReady()
  .then(async () => {
    console.log(
      JSON.stringify({
        check: 'desktop-relocated',
        versions: process.versions,
        executable: process.execPath,
        document: DOCUMENT_URL,
      }),
    );
    const { window } = await composeDesktop(undefined, false);
    assert(DOCUMENT_URL.includes('/moved/dist/renderer/index.html'));
    assert.deepEqual(
      await window.webContents.executeJavaScript('window.avencrew.describeCapabilities()'),
      { ok: false, code: 'DISCONNECTED' },
    );
    assert.equal(
      await window.webContents.executeJavaScript('window.avencrewBuild.appName'),
      'Avencrew',
    );
    assert.equal(
      await window.webContents.executeJavaScript('document.querySelector("h1").textContent'),
      'Avencrew',
    );
    window.destroy();
    clearTimeout(watchdog);
    console.log('moved bundle bridge and rendered metadata pass');
    app.exit(0);
  })
  .catch((error) => {
    console.error(String(error));
    app.exit(1);
  });
