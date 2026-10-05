//! Bounded agent execution harness.
//!
//! Intended to own exactly one reasoning/execution loop, shared by local runs,
//! cloud agents and worker jobs. That behaviour is P1-03/P3 work and is **not**
//! implemented here.
//!
//! # Dependency boundary
//!
//! Depends on `avencrew-contracts` only. It cannot reach the SQLite adapter,
//! because Cargo will not let it: `store-sqlite` is not in this crate's
//! dependency list. That is the enforcement mechanism, not a comment.
//!
//! Deliberately **not** present: a turn engine, model adapters, a tool dispatcher,
//! context assembly, compaction, or a stub agent. An empty loop that returns a
//! fabricated completion would be worse than no loop, because it would look like
//! a working harness. This crate reports build provenance and nothing else.

use avencrew_contracts::BuildInfo;

/// Build metadata for this crate.
pub const HARNESS: BuildInfo = BuildInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
