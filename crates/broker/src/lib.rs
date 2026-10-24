//! One sealed catalog and invocation path for trusted model, supervisor and
//! nested callers. Host ports own durable state, current authority and execution.
//! Model arguments never supply the trusted context.
mod bounds;
mod catalog;
mod invocation;
pub use catalog::{Binding, CatalogPage, Effect, SealedCatalog};
pub use invocation::{
    Broker, Dispatch, DispatchAdmission, Execution, Host, Invocation, Origin, Outcome,
    PolicySnapshot, PreparedInvocation,
};

/// Fixed errors contain no schema, arguments, source or provider diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    InvalidCatalog,
    InvalidArguments,
    LimitExceeded,
    UnsealedBinding,
    InvalidOrigin,
    IdempotencyMismatch,
    NotCommitted,
    StaleAuthority,
    Denied,
    Cancelled,
    Unsupported,
    InvalidResult,
    Storage,
    UnknownOutcome,
}
impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BrokerError {}
