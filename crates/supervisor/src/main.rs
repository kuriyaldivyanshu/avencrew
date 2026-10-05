//! Per-user Avencrew coordinator process.
//!
//! # What this binary does in this scaffold
//!
//! Reports build provenance and exits. That is all.
//!
//! It deliberately accepts **no** execution subcommands. A supervisor that
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
avencrew-supervisor — per-user coordinator (scaffold)

USAGE:
    avencrew-supervisor --version
    avencrew-supervisor --build-info

This scaffold builds and reports provenance only. It has no execution,
admission, scheduling, journal or environment behaviour yet.";

fn main() -> ExitCode {
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
