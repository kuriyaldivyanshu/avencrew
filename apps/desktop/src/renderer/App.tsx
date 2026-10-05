/**
 * Scaffold workspace view.
 *
 * Deliberate omissions, because their absence is the honest state:
 *   - no task list, because no task exists and none can be created yet
 *   - no activity feed, because nothing is scheduled or executing
 *   - no model, source, worker, connection or supervisor control
 *   - no disabled-looking "coming soon" buttons, which would imply a route that
 *     does not exist
 *
 * The empty areas say what is missing rather than implying quiet success.
 */

const NAVIGATION = ['Overview', 'Tasks', 'Activity'] as const;

function EmptyArea({ title, children }: { title: string; children: string }) {
  return (
    <section className="empty-area">
      <h2>{title}</h2>
      <p>{children}</p>
    </section>
  );
}

export function App() {
  const build = window.avencrewBuild;

  return (
    <div className="app">
      <header className="app-header">
        <h1>{build.appName}</h1>
        <p className="build-line">
          version {build.version} · {build.stage}
        </p>
      </header>

      <nav className="app-nav" aria-label="Local workspace sections">
        <ul>
          {NAVIGATION.map((item) => (
            <li key={item}>
              <span className="nav-item">{item}</span>
            </li>
          ))}
        </ul>
      </nav>

      <main className="app-main">
        <EmptyArea title="Local workspace">
          This window is a development scaffold. No workspace has been opened and
          no server is connected.
        </EmptyArea>
        <EmptyArea title="Tasks">
          None. Task admission, the coordinator and the harness are not implemented
          yet.
        </EmptyArea>
        <EmptyArea title="Activity">
          Nothing is scheduled or executing. There is no idle worker or agent
          watching anything.
        </EmptyArea>
      </main>
    </div>
  );
}