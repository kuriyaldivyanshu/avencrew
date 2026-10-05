// Build the three desktop bundles with esbuild.
//
// Separate outputs because they run in different worlds:
//   dist/main/main.js         Electron main, ESM, `electron` external
//   dist/preload/preload.cjs  preload, CommonJS (required by sandbox: true)
//   dist/renderer/index.js    renderer, browser ESM, React bundled in
//
// No dev server and no watch mode: `dev` is a build plus a normal launch, so a
// stale bundle can never be mistaken for current code.
import { build } from 'esbuild';
import { readFile, mkdir, copyFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const here = new URL('./', import.meta.url);
const pkg = JSON.parse(await readFile(new URL('./package.json', here), 'utf8'));
const dist = (path) => fileURLToPath(new URL(path, here));

const define = {
  __APP_VERSION__: JSON.stringify(pkg.version),
};

const shared = {
  bundle: true,
  define,
  target: 'es2022',
  logLevel: 'warning',
  legalComments: 'none',
};

await build({
  ...shared,
  entryPoints: [dist('src/main/main.ts')],
  outfile: dist('dist/main/main.js'),
  platform: 'node',
  format: 'esm',
  external: ['electron'],
});

await build({
  ...shared,
  entryPoints: [dist('src/preload/preload.ts')],
  outfile: dist('dist/preload/preload.cjs'),
  platform: 'node',
  // sandbox: true preloads are loaded as CommonJS; ESM is not supported there.
  format: 'cjs',
  external: ['electron'],
});

await build({
  ...shared,
  entryPoints: [dist('src/renderer/index.tsx')],
  outfile: dist('dist/renderer/index.js'),
  platform: 'browser',
  format: 'esm',
  jsx: 'automatic',
  loader: { '.css': 'css' },
});

// The HTML is copied verbatim; it is hand-written and must not be transformed,
// because its CSP is the renderer's security boundary.
await mkdir(dist('dist/renderer'), { recursive: true });
await copyFile(dist('src/renderer/index.html'), dist('dist/renderer/index.html'));

console.log('desktop build: dist/main, dist/preload, dist/renderer');