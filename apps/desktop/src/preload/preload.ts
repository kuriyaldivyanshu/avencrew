import { contextBridge, ipcRenderer } from 'electron';
import { CHANNELS, type DesktopBridge, PRELOAD_AUTHORIZE } from '../shared/bridge.js';
import { frozenBuildMetadata } from '../shared/build-metadata.js';

// The owning main process authorizes the current document at runtime, so the
// same built bundle remains valid after relocation. No page-visible raw IPC.
if (process.isMainFrame && ipcRenderer.sendSync(PRELOAD_AUTHORIZE) === true) {
  const bridge: DesktopBridge = {
    describeCapabilities: () => ipcRenderer.invoke(CHANNELS.describeCapabilities),
    steer: (input) => ipcRenderer.invoke(CHANNELS.steer, input),
    pause: (input) => ipcRenderer.invoke(CHANNELS.pause, input),
    stop: (input) => ipcRenderer.invoke(CHANNELS.stop, input),
  };
  contextBridge.exposeInMainWorld('avencrew', Object.freeze(bridge));
  contextBridge.exposeInMainWorld('avencrewBuild', frozenBuildMetadata());
}
