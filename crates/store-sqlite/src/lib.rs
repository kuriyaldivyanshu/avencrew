//! Local supervisor journal adapter.
//!
//! Intended to own the per-user SQLite journal: WAL, `synchronous=FULL`, STRICT
//! tables, one exclusive writer. That behaviour, and the controlled SQLite 3.53.4
//! packaging decision, are P2 work and are **not** implemented here.
//!
//! # Dependency boundary
//!
//! Depends on `avencrew-contracts` only. Nothing may depend on this crate except
//! the supervisor.
//!
//! Deliberately **not** present: a connection pool, a migration runner, table
//! definitions, or no-op CRUD methods. A journal adapter whose methods return
//! success without touching a database would be a false authority claim. This
//! crate reports build provenance and nothing else.

use avencrew_contracts::BuildInfo;

/// Build metadata for this crate.
pub const STORE_SQLITE: BuildInfo =
    BuildInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
