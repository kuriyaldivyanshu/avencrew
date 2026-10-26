import { app, BrowserWindow } from 'electron';
import { composeDesktop } from './composition.js';

// No renderer, environment or command-line launch configuration is consumed.
app.whenReady().then(async () => {
  await composeDesktop();
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) void composeDesktop();
  });
});
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
