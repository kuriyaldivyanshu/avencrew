/**
 * Preload bridge.
 *
 * Exposes exactly one frozen, static value. There is deliberately no
 * `ipcRenderer.invoke`, `ipcRenderer.send`, `ipcRenderer.on`, or any channel
 * name here. Adding one would turn the preload into a remote-control surface
 * for the renderer and is outside this scaffold.
 *
 * Built as CommonJS because `sandbox: true` preloads cannot be ES modules.
 */
import { contextBridge } from 'electron';

import { frozenBuildMetadata } from '../shared/build-metadata.js';

contextBridge.exposeInMainWorld('avencrewBuild', frozenBuildMetadata());
