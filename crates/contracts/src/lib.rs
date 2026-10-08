//! Canonical, provider-independent contracts for Avencrew.
//!
//! Boundary callers use [`decode_client_request`] or [`decode_internal_message`],
//! never unchecked `serde_json::from_slice`. Validation does not authenticate a
//! caller, grant a lease, or establish a broker permission decision.
//! Contracts have no dependency on harness, storage, Electron, or providers.
pub mod bundles;
pub mod framing;
mod json;
pub mod scalars;
mod semantics;
pub mod wire;

pub use json::{canonical_bytes, decode_client_request, decode_internal_message, encode_message};

/// Bounded, payload-free errors safe to report at a transport boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractError {
    InvalidMessage,
    IncompatibleVersion,
    InvalidScalar(&'static str),
    Limit(&'static str),
    InvalidUnion(&'static str),
    UntrustedOperation,
    IncompleteFrame,
    DecoderFailed,
    NonIntegralCanonicalNumber,
}
impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMessage => f.write_str("invalid JSON or wire shape"),
            Self::IncompatibleVersion => f.write_str("unsupported protocol version"),
            Self::InvalidScalar(name) => write!(f, "invalid {name}"),
            Self::Limit(name) => write!(f, "exceeded {name}"),
            Self::InvalidUnion(name) => write!(f, "inconsistent {name}"),
            Self::UntrustedOperation => f.write_str("operation is not a client operation"),
            Self::IncompleteFrame => f.write_str("incomplete frame"),
            Self::DecoderFailed => f.write_str("decoder closed after an invalid frame"),
            Self::NonIntegralCanonicalNumber => {
                f.write_str("canonical records require integral JSON numbers")
            }
        }
    }
}
impl std::error::Error for ContractError {}

/// Structural/portable semantic validation, independent of current authority.
pub trait Validate {
    fn validate(&self) -> Result<(), ContractError>;
}
impl Validate for bool {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}
impl Validate for serde_json::Value {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

/// JSON Schema 2020-12 generated from the canonical Rust message definitions.
/// Counter range/calendar and cross-field rules additionally require `Validate`.
#[cfg(feature = "schema-export")]
pub fn wire_schema() -> schemars::Schema {
    let mut schema = schemars::generate::SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<wire::WireMessage>();
    schema.insert("$id".into(), serde_json::json!("urn:avencrew:wire:1.0"));
    schema.insert("$comment".into(), serde_json::json!("Structural schema. Also enforce signed-64 counter bounds, valid UTC calendar instants, environment/generation pairs, cursor order, committed transfer consistency, duplicate-key rejection and transport/client authority boundaries."));
    schema
}

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

impl<T: Validate + ?Sized> Validate for Box<T> {
    fn validate(&self) -> Result<(), ContractError> {
        (**self).validate()
    }
}
