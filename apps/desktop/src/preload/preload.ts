import { contextBridge, ipcRenderer } from 'electron';
import { CHANNELS, type DesktopBridge } from '../shared/bridge.js';
import { frozenBuildMetadata } from '../shared/build-metadata.js';

// Electron's sandbox preload has a deliberately restricted Node surface. The
// fixed URL is compiled by build.mjs, never supplied through renderer arguments.
declare const location: { readonly href: string };
declare const __TRUSTED_DOCUMENT_URL__: string;
if (process.isMainFrame && location.href === __TRUSTED_DOCUMENT_URL__) {
  const bridge: DesktopBridge = {
    describeCapabilities: () => ipcRenderer.invoke(CHANNELS.describeCapabilities),
    steer: (input) => ipcRenderer.invoke(CHANNELS.steer, input),
    pause: (input) => ipcRenderer.invoke(CHANNELS.pause, input),
    stop: (input) => ipcRenderer.invoke(CHANNELS.stop, input),
  };
  contextBridge.exposeInMainWorld('avencrew', Object.freeze(bridge));
  contextBridge.exposeInMainWorld('avencrewBuild', frozenBuildMetadata());
}
