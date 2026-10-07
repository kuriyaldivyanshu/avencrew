//! Engine diagnostic for the controlled native SQLite build.
//!
//! Opens a real SQLx connection to an in-memory database and verifies that the
//! linked engine is the pinned amalgamation with the reviewed compile options.
//!
//! This is a **diagnostic**, not journal behaviour. It creates no user database,
//! no journal schema and no placeholder CRUD API. Migrations, disk/WAL/FULL
//! writer ownership, backup and crash behaviour are P2.
//!
//! Safe Rust only: no direct native FFI. Any failed expectation exits nonzero so
//! a wrong engine, a dynamic link or a weakened compile option cannot pass
//! silently.

use std::process::ExitCode;

use sqlx::sqlite::SqliteConnection;
use sqlx::{Connection, Row};

/// Pinned engine version, supplied by the build environment so this file does
/// not duplicate tools/bootstrap/pins.json.
const EXPECTED_VERSION: &str = env!("AVENCREW_SQLITE_VERSION");

/// Pinned source ID of the selected amalgamation.
const EXPECTED_SOURCE_ID: &str =
    "2026-07-24 19:02:57 bf7c7f30031888f4e796e429ab3978879485813aaca6f641c7b33e4e09459bcc";

/// Compile options the reviewed recipe requires, as (needle, description).
///
/// These are the names the engine actually reports through `PRAGMA
/// compile_options`, which differ from the `-D` spellings: `SQLITE_ENABLE_FTS5`
/// is reported as `ENABLE_FTS5`, `SQLITE_OMIT_LOAD_EXTENSION` as
/// `OMIT_LOAD_EXTENSION`, and so on. Matching the reported form means this
/// checks the linked binary, not our own flag string.
const REQUIRED_OPTIONS: &[(&str, &str)] = &[
    ("ENABLE_FTS5", "FTS5 full-text search"),
    ("ENABLE_COLUMN_METADATA", "column metadata API"),
    ("OMIT_LOAD_EXTENSION", "extension loading omitted"),
    ("THREADSAFE=1", "thread-safe engine build"),
    // Durability defaults are compiled in as defense in depth. P2 still
    // configures and verifies every connection explicitly.
    ("DEFAULT_FOREIGN_KEYS", "foreign keys default on"),
    ("DEFAULT_SYNCHRONOUS=2", "synchronous default FULL"),
    ("DEFAULT_WAL_SYNCHRONOUS=2", "WAL synchronous default FULL"),
];

fn check(condition: bool, description: &str) -> Result<(), String> {
    if condition {
        println!("  ok    {description}");
        Ok(())
    } else {
        Err(format!("FAILED: {description}"))
    }
}

async fn run() -> Result<(), String> {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .map_err(|error| format!("could not open an in-memory SQLite connection: {error}"))?;
    println!("connection: in-memory");

    // 1. Identity: version and source ID must match the pinned amalgamation.
    let version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(&mut connection)
        .await
        .map_err(|error| format!("sqlite_version() failed: {error}"))?;
    println!("sqlite_version   = {version}");
    check(
        version == EXPECTED_VERSION,
        &format!("sqlite_version is {EXPECTED_VERSION}"),
    )?;

    let source_id: String = sqlx::query_scalar("SELECT sqlite_source_id()")
        .fetch_one(&mut connection)
        .await
        .map_err(|error| format!("sqlite_source_id() failed: {error}"))?;
    println!("sqlite_source_id = {source_id}");
    check(
        source_id == EXPECTED_SOURCE_ID,
        &format!("sqlite_source_id matches the pinned amalgamation {EXPECTED_SOURCE_ID}"),
    )?;

    // 2. Compile options actually compiled into the linked engine.
    let rows = sqlx::query("PRAGMA compile_options")
        .fetch_all(&mut connection)
        .await
        .map_err(|error| format!("PRAGMA compile_options failed: {error}"))?;
    let mut options: Vec<String> = Vec::with_capacity(rows.len());
    for row in &rows {
        let option: String = row
            .try_get(0)
            .map_err(|error| format!("reading compile_options failed: {error}"))?;
        options.push(option);
    }
    println!("compile_options  = {} entries", options.len());
    for (needle, description) in REQUIRED_OPTIONS {
        let present = options.iter().any(|option| option == needle);
        check(
            present,
            &format!("compile_options contains {needle} ({description})"),
        )?;
    }

    // 3. JSON is built into the selected source; we did not omit it.
    let json: String = sqlx::query_scalar("SELECT json_object('key', 'value')")
        .fetch_one(&mut connection)
        .await
        .map_err(|error| format!("JSON function failed: {error}"))?;
    check(
        json == "{\"key\":\"value\"}",
        &format!("JSON result is correct ({json})"),
    )?;

    // 4. FTS5 really works, not merely compiled in.
    sqlx::query("CREATE VIRTUAL TABLE docs USING fts5(body)")
        .execute(&mut connection)
        .await
        .map_err(|error| format!("creating an FTS5 table failed: {error}"))?;
    sqlx::query(
        "INSERT INTO docs(body) VALUES ('export totals were wrong'), ('progress bar please')",
    )
    .execute(&mut connection)
    .await
    .map_err(|error| format!("inserting into FTS5 failed: {error}"))?;
    let hit: String = sqlx::query_scalar("SELECT body FROM docs WHERE docs MATCH 'export'")
        .fetch_one(&mut connection)
        .await
        .map_err(|error| format!("FTS5 MATCH failed: {error}"))?;
    check(
        hit == "export totals were wrong",
        &format!("FTS5 MATCH returns the expected row ({hit})"),
    )?;

    // 5. STRICT tables reject a wrongly typed value.
    sqlx::query("CREATE TABLE strict_probe(value INTEGER) STRICT")
        .execute(&mut connection)
        .await
        .map_err(|error| format!("creating a STRICT table failed: {error}"))?;
    let strict_rejected = sqlx::query("INSERT INTO strict_probe(value) VALUES ('not an integer')")
        .execute(&mut connection)
        .await
        .is_err();
    check(
        strict_rejected,
        "STRICT rejects a text value in an INTEGER column",
    )?;

    // 6. Extension loading must be unavailable. The build omits it, so the
    //    function should not resolve at all.
    let extension_refused = sqlx::query("SELECT load_extension('/nonexistent')")
        .fetch_one(&mut connection)
        .await
        .is_err();
    check(
        extension_refused,
        "load_extension() is unavailable (SQLITE_OMIT_LOAD_EXTENSION)",
    )?;

    Ok(())
}

fn main() -> ExitCode {
    // Current-thread runtime builder; no #[tokio::main] macro requirement.
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("could not build a Tokio runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(run()) {
        Ok(()) => {
            println!("\nengine diagnostic passed");
            ExitCode::SUCCESS
        }
        Err(problem) => {
            eprintln!("\nengine diagnostic failed\n  {problem}");
            ExitCode::FAILURE
        }
    }
}
