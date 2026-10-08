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
    avencrew-supervisor diagnostics --data-root ABSOLUTE_DIRECTORY
    avencrew-supervisor backup --data-root ABSOLUTE_DIRECTORY --destination ABSOLUTE_NEW_DIRECTORY --backup-id UUIDV7 --at UTC_TIMESTAMP
    avencrew-supervisor backup-check --source ABSOLUTE_DIRECTORY
    avencrew-supervisor restore-check --source ABSOLUTE_DIRECTORY --destination ABSOLUTE_NEW_DIRECTORY

store-check boots and validates the durable SQLite journal, then closes it.
serve requires a private, framed main-session launch context on stdin/stdout.
Backups contain private data; they are not redacted diagnostics or encrypted exports.
restore-check copies into a fresh quarantined directory; ordinary boot remains blocked.
No executable task admission, harness, scheduling or environment behavior yet.";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("backup" | "backup-check" | "restore-check")
    ) {
        return backup_command(&args);
    }
    if args.first().map(String::as_str) == Some("diagnostics") {
        return diagnostics(&args[1..]);
    }
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

/// Explicit local export; no upload, log concatenation or arbitrary row dump.
fn diagnostics(args: &[String]) -> ExitCode {
    if args.len() != 2 || args[0] != "--data-root" || !std::path::Path::new(&args[1]).is_absolute()
    {
        eprintln!("usage: avencrew-supervisor diagnostics --data-root ABSOLUTE_DIRECTORY");
        return ExitCode::from(2);
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("diagnostics: runtime unavailable");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(async {
        let path = std::path::Path::new(&args[1]);
        // Diagnostics never create a missing root or upgrade a database. Boot
        // checks are required; only the currently supported schema is admitted.
        let mut store =
            avencrew_store_sqlite::LocalStore::open_existing_for_diagnostics(path).await?;
        let result = store.diagnostic_bytes().await;
        let closed = store.close().await;
        match (result, closed) {
            (Ok(bytes), Ok(())) => Ok(bytes),
            (Err(e), _) | (_, Err(e)) => Err(e),
        }
    });
    match result {
        Ok(bytes) => {
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            if stdout
                .write_all(&bytes)
                .and_then(|_| stdout.write_all(b"\n"))
                .is_err()
            {
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(_) => {
            eprintln!("diagnostics: verified local store unavailable");
            ExitCode::FAILURE
        }
    }
}

// Keep the command grammar exact: no repeated flags, force/overwrite or activation.
fn backup_command(args: &[String]) -> ExitCode {
    use avencrew_contracts::scalars::{DomainId, Instant};
    use avencrew_store_sqlite::{BackupError, LocalStore};
    use std::path::Path;
    let kind = args[0].as_str();
    let valid = match kind {
        "backup" => {
            args.len() == 9
                && args[1] == "--data-root"
                && args[3] == "--destination"
                && args[5] == "--backup-id"
                && args[7] == "--at"
                && Path::new(&args[2]).is_absolute()
                && Path::new(&args[4]).is_absolute()
        }
        "backup-check" => {
            args.len() == 3 && args[1] == "--source" && Path::new(&args[2]).is_absolute()
        }
        "restore-check" => {
            args.len() == 5
                && args[1] == "--source"
                && args[3] == "--destination"
                && Path::new(&args[2]).is_absolute()
                && Path::new(&args[4]).is_absolute()
        }
        _ => false,
    };
    if !valid {
        eprintln!("backup: invalid invocation; see supervisor help");
        return ExitCode::from(2);
    }
    let allocated = if kind == "backup" {
        match (
            DomainId::new(args[6].clone()),
            Instant::new(args[8].clone()),
        ) {
            (Ok(id), Ok(at)) => Some((id, at)),
            _ => {
                eprintln!("backup: invalid ID or timestamp");
                return ExitCode::from(2);
            }
        }
    } else {
        None
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(_) => {
            eprintln!("backup: runtime unavailable");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(async {
        let encode = |value: serde_json::Value| {
            avencrew_contracts::canonical_bytes(
                &serde_json::to_vec(&value).map_err(|_| BackupError::InvalidManifest)?,
            )
            .map_err(|_| BackupError::InvalidManifest)
        };
        if let Some((id, at)) = allocated {
            let mut store = LocalStore::open_existing_for_diagnostics(Path::new(&args[2]))
                .await
                .map_err(BackupError::Store)?;
            let result = store.create_backup(Path::new(&args[4]), &id, &at).await;
            let closed = store.close().await.map_err(BackupError::Store);
            let receipt = match (result, closed) {
                (Ok(r), Ok(())) => r,
                (Err(e), _) | (_, Err(e)) => return Err(e),
            };
            encode(serde_json::json!({"disposition":"verified_private_backup","receipt":receipt}))
        } else if kind == "backup-check" {
            let inspection = LocalStore::inspect_backup(Path::new(&args[2])).await?;
            encode(serde_json::to_value(inspection).map_err(|_| BackupError::InvalidManifest)?)
        } else {
            let receipt =
                LocalStore::restore_backup_quarantined(Path::new(&args[2]), Path::new(&args[4]))
                    .await?;
            encode(serde_json::to_value(receipt).map_err(|_| BackupError::InvalidManifest)?)
        }
    });
    match result {
        Ok(bytes) => {
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            if out
                .write_all(&bytes)
                .and_then(|_| out.write_all(b"\n"))
                .is_err()
            {
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("backup: {e}");
            ExitCode::FAILURE
        }
    }
}
