/**
 * The only value the preload bridge adds to the renderer's global scope.
 *
 * Declared in renderer-owned code so the main/preload type environment never
 * has to know about `Window` (its `lib` has no DOM at all).
 */
import type { AppBuildMetadata } from '../shared/build-metadata.js';

declare global {
  interface Window {
    readonly avencrewBuild: AppBuildMetadata;
  }
}

export {};