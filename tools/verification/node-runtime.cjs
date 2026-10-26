// Optional check-scoped NODE_OPTIONS preload: records the executing process,
// without arguments, environment, protocol frames or application credentials.
console.error(JSON.stringify({ check: 'node-runtime', node: process.version, executable: process.execPath, entry: process.argv[1] }));
