const { contextBridge, ipcRenderer } = require('electron');
contextBridge.exposeInMainWorld('rawInvoke', (channel, ...args) =>
  ipcRenderer.invoke(channel, ...args),
);

contextBridge.exposeInMainWorld('boundaryProbe', {
  sandboxed: process.sandboxed,
  contextIsolated: process.contextIsolated,
  isMainFrame: process.isMainFrame,
});
