import { build } from 'esbuild';
import { copyFile, mkdir } from 'node:fs/promises';
console.log(
  JSON.stringify({
    check: 'desktop-test-build',
    node: process.version,
    executable: process.execPath,
  }),
);
await build({
  entryPoints: ['tests/native.ts'],
  bundle: true,
  platform: 'node',
  format: 'esm',
  external: ['electron'],
  outfile: 'dist/main/native-test.js',
});
await build({
  entryPoints: ['tests/relocated.ts'],
  bundle: true,
  platform: 'node',
  format: 'esm',
  external: ['electron'],
  outfile: 'dist/main/relocation-test.js',
});
await build({
  entryPoints: ['src/main/client.ts', 'src/main/transport.ts'],
  splitting: true,
  bundle: true,
  platform: 'node',
  format: 'esm',
  outdir: 'dist/tests',
});
await mkdir('dist/preload', { recursive: true });
await copyFile('tests/hostile.cjs', 'dist/preload/hostile.cjs');
