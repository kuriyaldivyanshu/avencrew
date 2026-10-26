# Avencrew desktop

The desktop exposes a narrow supervisor control boundary. The production entry
composes a disconnected bridge; task creation, execution screens, workspace
admission, reconnect and quit/background policy remain later work (P4-08/P4-09).

Use the existing project-managed toolchain and installed pinned dependencies:

```sh
bash tools/bootstrap/dev-env.sh pnpm --filter @avencrew/desktop typecheck
bash tools/bootstrap/dev-env.sh pnpm --filter @avencrew/desktop build
bash tools/bootstrap/dev-env.sh pnpm --filter @avencrew/desktop test
bash tools/bootstrap/dev-env.sh pnpm --filter @avencrew/desktop test:native
```

`test:native` executes only on macOS ARM64 with the pinned Electron distribution.
It launches hidden sandboxed windows, a real supervisor and a throwaway registered
queued run, then checks accepted durable receipts after reopening the store.
It explicitly kills its owned fixture supervisor after checking that window/client
close leaves it alive. Linux native Electron remains unverified. The ordinary
transport tests include real deadlines and synthetic Unix-socket malformed peers;
they require permission to bind local sockets. The `desktop` and `all` checks
include these unit tests. CI separately gates native Electron on macOS; the
relocated-bundle smoke is a scratch copy test, not release packaging evidence. No test installs dependencies.
The shared verification lanes are `desktop`, `desktop-unit` and `desktop-native`.

`window.avencrew` has exactly four named methods: `describeCapabilities()`,
`steer(input)`, `pause(input)` and `stop(input)`. Inputs use generated command
variants with `kind` omitted; main supplies and validates that discriminator.
Results contain the original validated capability/receipt, or a fixed safe failure
code. Valid server rejection returns `REJECTED`; connection failure returns
`UNAVAILABLE`. Every receipt must match the submitted command and, when present,
run identity. An accepted receipt does not imply application. Explicit retries preserve
command identity; no mutation is retried automatically. Static build metadata is
separately exposed as `window.avencrewBuild`.

Every main IPC handler checks its owning WebContents, current main frame and exact
fixed document URL before validating input. The sandboxed preload synchronously asks a narrow internal authorization
channel for a boolean under that same guard, then exposes the bridge only on the
main document. No build-machine document path is embedded; copied bundles
resolve their local document at runtime. Node in pages/workers/subframes is disabled;
context isolation and the renderer sandbox remain enabled. Navigation, external
opening, popups, webviews, downloads and permissions are denied. There is no raw
IPC, general RPC, shell, filesystem, SQL or credential API in the page.

`composition.ts` accepts optional trusted main-only launch configuration. Production
reads no configuration or credentials from renderer input, environment or argv.
The native test is a separate entry that invokes this same composition and client.
The client launches `serve` with anonymous pipes and `shell:false`, checks a bounded
strict launch grant and authenticates the fresh connection challenge using the
existing canonical HMAC transcript. Credentials remain main memory/pipe data.
Trusted roots are canonicalized before launch. Failed bootstrap terminates and
awaits only its newly spawned child; successful authentication transfers to an
independent lifetime, so ordinary disconnect never sends a signal.
Frames are capped at 1 MiB, pending requests at 16; launch/auth/partial-read/write
deadlines are five seconds and request deadlines ten seconds. Failure closes the
connection and rejects pending calls without fabricating acceptance.

`windows.ts` also owns the production artifact preview policy: scripts disabled,
no preload, separate in-memory session, and network/download/navigation denial.
The native test consumes this exact policy with hostile HTML. Real artifact
fetching, audience checks and artifact-version review are deferred; V-U06 remains
partial. The preview factory alone is not a complete artifact workflow.

The bridge follows Electron's [security guidance](https://www.electronjs.org/docs/latest/tutorial/security),
[named context-isolated APIs](https://www.electronjs.org/docs/latest/tutorial/context-isolation)
and [process model](https://www.electronjs.org/docs/latest/tutorial/process-model).
Installed Electron 44.5.1 declarations are the API authority for this build.
