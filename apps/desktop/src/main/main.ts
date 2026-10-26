import { app, BrowserWindow } from 'electron';
import { composeDesktop } from './composition.js';

// No renderer, environment or command-line launch configuration is consumed.
const openDesktop = async (): Promise<void> => {
  try {
    await composeDesktop();
  } catch {
    console.error('Desktop unavailable');
  }
};
void app
  .whenReady()
  .then(async () => {
    await openDesktop();
    app.on('activate', () => {
      if (BrowserWindow.getAllWindows().length === 0) void openDesktop();
    });
  })
  .catch(() => console.error('Desktop unavailable'));
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
