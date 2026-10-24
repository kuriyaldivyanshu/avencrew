//! Per-user Avencrew coordinator process.
//!
//! Reports build provenance, manages the durable store and authenticated control
//! socket, and runs an explicit finite synthetic fixture through the owned
//! engine, broker and journal. Production admission and environments are disabled.
//!
//! # Dependency boundary
//!
//! The supervisor composes the portable engine/broker with SQLite and OS clocks;
//! neither the store nor portable harness acquires a supervisor dependency.

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
    avencrew-supervisor fixture-run --data-root EMPTY_TEMP_DIRECTORY --input FIXTURE_JSON
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
fixture-run executes only zero-priced synthetic model/source inputs in a fresh temporary root.
Production task admission, scheduling and execution environments remain disabled.";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("fixture-run") {
        return fixture_run(&args[1..]);
    }
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

fn fixture_run(args: &[String]) -> ExitCode {
    use avencrew_supervisor::execution::{
        execute_fixture_turn, prepare_fixture_turn, FixtureLaunch,
    };
    use std::{fs, io::Read, path::Path, sync::Arc};
    use tokio::sync::Mutex;
    if args.len() != 4 || args[0] != "--data-root" || args[2] != "--input" {
        eprintln!("usage: avencrew-supervisor fixture-run --data-root EMPTY_TEMP_DIRECTORY --input FIXTURE_JSON");
        return ExitCode::from(2);
    }
    let input = (|| -> Result<(std::path::PathBuf, FixtureLaunch), Box<dyn std::error::Error>> {
        let root = Path::new(&args[1]);
        let metadata = fs::symlink_metadata(root)?;
        if !root.is_absolute() || !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("fixture root must be a fresh temporary directory".into());
        }
        let root = root.canonicalize()?;
        let temp = std::env::temp_dir().canonicalize()?;
        if root == temp || !root.starts_with(temp) || fs::read_dir(&root)?.next().is_some() {
            return Err("fixture root must be an empty temporary subdirectory".into());
        }
        let file = fs::File::open(&args[3])?;
        let mut bytes = Vec::new();
        file.take(1_048_577).read_to_end(&mut bytes)?;
        let value = avencrew_contracts::decode_json(&bytes)?;
        let launch: FixtureLaunch = serde_json::from_value(value)?;
        if launch.schema_version != "avencrew.fixture-launch/1"
            || launch.profile.provider != "fixture"
            || launch.profile.rate_micro != "0"
        {
            return Err("fixture profile unavailable".into());
        }
        Ok((root, launch))
    })();
    let (root, launch) = match input {
        Ok(input) => input,
        Err(_) => {
            eprintln!("fixture-run: invalid fixture or temporary root; nothing executed");
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(_) => {
            eprintln!("fixture-run: runtime unavailable");
            return ExitCode::FAILURE;
        }
    };
    let result=runtime.block_on(async{
        let mut store=avencrew_store_sqlite::LocalStore::open(&root).await.map_err(|_|())?;
        let prepared=match prepare_fixture_turn(&mut store,&launch).await{Ok(p)=>p,Err(_)=>{let _=store.close().await;return Err(());}};
        let store=Arc::new(Mutex::new(store));
        let outcome=execute_fixture_turn(store.clone(),prepared.workspace.clone(),prepared.actor.clone(),prepared.turn.clone(),launch.profile,avencrew_harness::engine::EngineConfig::default()).await;
        let budget=store.lock().await.local_execution_budget(&prepared.workspace,&prepared.actor,&prepared.turn).await;
        let store=Arc::try_unwrap(store).map_err(|_|())?.into_inner();store.close().await.map_err(|_|())?;
        Ok::<_,()>(serde_json::json!({"schema_version":"avencrew.fixture-run/1","profile":"fixture_only","workspace_id":prepared.workspace,"actor_id":prepared.actor,"turn_id":prepared.turn,"outcome":outcome.map_err(|_|())?,"budget":budget.map_err(|_|())?}))
    });
    match result {
        Ok(report) => {
            match avencrew_contracts::canonical_bytes(
                &serde_json::to_vec(&report).unwrap_or_default(),
            ) {
                Ok(bytes) => {
                    println!("{}", String::from_utf8_lossy(&bytes));
                    ExitCode::SUCCESS
                }
                Err(_) => {
                    eprintln!("fixture-run: report unavailable");
                    ExitCode::FAILURE
                }
            }
        }
        Err(()) => {
            eprintln!(
                "fixture-run: no retained outcome acknowledged; journal preserved for inspection"
            );
            ExitCode::FAILURE
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
