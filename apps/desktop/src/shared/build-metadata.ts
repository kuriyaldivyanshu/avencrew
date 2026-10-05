/**
 * Frozen, static build metadata shared by the preload bridge and the renderer.
 *
 * This is deliberately the *only* thing the preload exposes. It carries no
 * capability: no command channel, no filesystem or path API, no SQL, no secrets,
 * no supervisor control.
 *
 * It is build provenance for the scaffold, not a domain contract and not a
 * substitute for the canonical wire contracts owned by P1-03/P1-04.
 */

/** Injected at bundle time by build.mjs. Declared so `tsc` can check callers. */
declare const __APP_VERSION__: string;

export interface AppBuildMetadata {
  /** Product name shown in the renderer header. */
  readonly appName: 'Avencrew';
  /** Version from apps/desktop/package.json, injected at build time. */
  readonly version: string;
  /**
   * Honest scaffold state. The renderer uses this to explain why task and
   * activity areas are empty instead of implying work is in progress.
   */
  readonly stage: 'scaffold';
}

/**
 * Build the frozen metadata object.
 *
 * `Object.freeze` matters: contextBridge copies this across the isolation
 * boundary, and the renderer must not be able to mutate what it was given.
 */
export function frozenBuildMetadata(): AppBuildMetadata {
  return Object.freeze({
    appName: 'Avencrew',
    version: __APP_VERSION__,
    stage: 'scaffold',
  } satisfies AppBuildMetadata);
}