import type { DesktopBridge } from '../shared/bridge.js';
import type { AppBuildMetadata } from '../shared/build-metadata.js';

declare global {
  interface Window {
    readonly avencrewBuild: AppBuildMetadata;
    readonly avencrew: DesktopBridge;
  }
}
