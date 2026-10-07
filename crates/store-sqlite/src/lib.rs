//! Local SQLite journal boot; supervisor is the sole owner.
//! No public raw SQL, pool, admission or harness execution surface.

mod journal;
pub use journal::{
    BundlePublication, LocalStore, Payload, PublicationReceipt, PublishedBundle, RecordAllocation,
    RegistrationReceipt, StoreError, StoreStatus,
};

use avencrew_contracts::BuildInfo;
pub const STORE_SQLITE: BuildInfo =
    BuildInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
