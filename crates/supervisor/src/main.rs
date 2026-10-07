//! Per-user Avencrew coordinator process.
//!
//! # What this binary does in this scaffold
//!
//! Reports build provenance, boots/checks/closes the local durable store, or owns
//! the authenticated local control socket. Task execution remains disabled.
//!
//! It deliberately accepts **no** task execution subcommands. A supervisor that
//! answered `run` or `submit` without a coordinator, a journal or a harness would
//! be claiming authority it does not have. An honest "not implemented" is the
//! correct scaffold behaviour; see the batch rule against fake run completions,
//! no-op database methods and stub agents.
//!
//! # Dependency boundary
//!
//! This is the only crate that depends on all three others, so it is the only
//! place that can report whole-workspace build state. Consuming each component
//! here is what makes those dependency edges real rather than decorative.

use std::process::ExitCode;

use avencrew_contracts::BuildInfo;
use avencrew_harness::HARNESS;
use avencrew_store_sqlite::STORE_SQLITE;

/// Build metadata for this binary.
const SUPERVISOR: BuildInfo = BuildInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

/// Every component in the workspace, in dependency order.
const COMPONENTS: [BuildInfo; 4] = [
    SUPERVISOR,
    avencrew_contracts::CONTRACTS,
    HARNESS,
    STORE_SQLITE,
];

/// Command used when no arguments are given.
const USAGE: &str = "\
avencrew-supervisor — per-user coordinator foundation

USAGE:
    avencrew-supervisor --version
    avencrew-supervisor --build-info
    avencrew-supervisor store-check --data-root ABSOLUTE_DIRECTORY
    avencrew-supervisor serve --data-root ABSOLUTE_DIRECTORY

store-check boots and validates the durable SQLite journal, then closes it.
serve requires a private, framed main-session launch context on stdin/stdout.
No executable task admission, harness, scheduling or environment behavior yet.";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("serve") {
        return serve(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("store-check") {
        return store_check(&args[1..]);
    }
    let mut arguments = std::env::args().skip(1);

    match (arguments.next(), arguments.next()) {
        (None, _) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        (Some(flag), None) if flag == "--version" => {
            println!("{SUPERVISOR}");
            ExitCode::SUCCESS
        }
        (Some(flag), None) if flag == "--build-info" => {
            for component in COMPONENTS {
                println!("{component}");
            }
            ExitCode::SUCCESS
        }
        (Some(unknown), _) => {
            eprintln!("avencrew-supervisor: unsupported argument {unknown:?}");
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn store_check(args: &[String]) -> ExitCode {
    if args.len() != 2 || args[0] != "--data-root" {
        eprintln!("usage: avencrew-supervisor store-check --data-root ABSOLUTE_DIRECTORY");
        return ExitCode::from(2);
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("supervisor runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(async {
        let store = avencrew_store_sqlite::LocalStore::open(std::path::Path::new(&args[1])).await?;
        let status = store.status().clone();
        store.close().await?;
        Ok::<_, avencrew_store_sqlite::StoreError>(status)
    });
    match result {
        Ok(status) => {
            println!("journal verified: schema {} / {} domain tables / WAL / FULL / foreign keys / 5000ms busy timeout", status.schema_version, status.domain_tables);
            println!("database: {}", status.database_path.display());
            println!("Journal boot only; no task or harness executed.");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn serve(args: &[String]) -> ExitCode {
    if args.len() != 2 || args[0] != "--data-root" {
        eprintln!("usage: avencrew-supervisor serve --data-root ABSOLUTE_DIRECTORY");
        return ExitCode::from(2);
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(_) => {
            eprintln!("supervisor runtime unavailable");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(async {
        let context = avencrew_supervisor::read_launch_context()?;
        let supervisor =
            avencrew_supervisor::Supervisor::open(std::path::Path::new(&args[1]), context).await?;
        avencrew_supervisor::write_launch_grant(supervisor.grant())?;
        let (owner_shutdown, shutdown) = tokio::sync::oneshot::channel();
        // Connection/window EOF is not shutdown. The trusted process owner may stop
        // this process; explicit ordered service shutdown/recovery follows in P2-05.
        let result = supervisor.serve(shutdown).await;
        drop(owner_shutdown);
        result
    });
    if let Err(error) = result {
        eprintln!("supervisor: {error}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
