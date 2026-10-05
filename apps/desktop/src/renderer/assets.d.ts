/**
 * Ambient declarations for non-code imports handled by the bundler.
 *
 * esbuild turns `import './app.css'` into a stylesheet emit and
 * `import './x.svg'` into a URL. TypeScript needs to be told these resolve, and
 * the renderer config has `types: []`, so nothing else declares them.
 */

declare module '*.css' {
  const content: string;
  export default content;
}

declare module '*.svg' {
  const url: string;
  export default url;
}