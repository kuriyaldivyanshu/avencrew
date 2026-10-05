//! Shared contract surface for the Avencrew runtime.
//!
//! This crate sits at the bottom of the workspace dependency graph and depends
//! on nothing, internal or external. That is deliberate: it is the only place a
//! type may live if every other crate is to agree on it, so it must not be able
//! to reach provider, Electron, harness or storage types at all.
//!
//! # Scope in this scaffold
//!
//! Only build metadata is defined here. Canonical wire contracts, DTOs and their
//! code generation are Sol-owned P1-03/P1-04 work and are deliberately absent.
//! Nothing in this crate serialises, transports, or authorises anything.
//!
//! Deliberately **not** present, and not to be added speculatively:
//! run/attempt/task records, tool envelopes, approval bindings, credential
//! handles, or any provider-shaped error taxonomy. Those belong to the frozen
//! contract documents, not to a placeholder type.

/// Identity and version of one workspace component.
///
/// Development scaffold metadata, reported by the supervisor CLI so that a build
/// can be identified. This is build provenance, not a domain contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildInfo {
    /// Crate name, as published in the workspace.
    pub component: &'static str,
    /// Crate version from the workspace manifest.
    pub version: &'static str,
}

impl BuildInfo {
    /// Create build metadata for a component.
    pub const fn new(component: &'static str, version: &'static str) -> Self {
        Self { component, version }
    }
}

impl core::fmt::Display for BuildInfo {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{} {}", self.component, self.version)
    }
}

/// Build metadata for this crate.
pub const CONTRACTS: BuildInfo = BuildInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
