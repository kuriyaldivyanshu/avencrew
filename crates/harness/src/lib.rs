//! Bounded agent execution harness.
//!
//! The bounded turn engine consumes normalized model responses through one
//! sealed invocation broker. Trusted host ports own persistence and authority.
//! The fixture consumer exercises this loop; production coordinator composition
//! and live inference are not enabled. Model final text is only a candidate.
//!
//! # Dependency boundary
//!
//! It depends on contracts and the broker. It cannot reach SQLite,
//! because Cargo will not let it: `store-sqlite` is not in this crate's
//! dependency list. That is the enforcement mechanism, not a comment.
//!
//! Fixture context compaction preserves exact source proofs; live model adapters remain later work.

pub mod artifacts;
pub mod engine;
pub mod model;

use avencrew_contracts::BuildInfo;

/// Build metadata for this crate.
pub const HARNESS: BuildInfo = BuildInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
