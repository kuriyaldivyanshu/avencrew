//! Local SQLite journal boot; supervisor is the sole owner.
//! No public raw SQL, pool, admission or harness execution surface.

mod journal;
pub use journal::{
    BackupError, BackupInspection, BackupReceipt, BlobImport, BundlePublication, CheckpointFile,
    CheckpointPublication, CheckpointReceipt, CommandAcceptance, ControlError, DiagnosticCounts,
    FileCoverage, LocalArtifact, LocalArtifactPublication, LocalArtifactReceipt,
    LocalArtifactTarget, LocalControlSnapshot, LocalDiagnostics, LocalDispatch,
    LocalExecutionAccess, LocalExecutionBudget, LocalExecutionCheckpoint, LocalExecutionRestore,
    LocalExecutionSnapshot, LocalRunReceipt, LocalRunRegistration, LocalStore,
    LocalTurnPublication, LocalTurnReceipt, Payload, PublicationReceipt, PublishedBundle,
    RecordAllocation, RecoveredControl, RecoveredRun, RecoveryGate, RecoverySnapshot,
    RegistrationReceipt, RestoreReceipt, RestoredCheckpoint, RetainedLocalTurn, StagingCleanup,
    StoreError, StoreStatus, TurnError, VaultReceipt,
};

use avencrew_contracts::BuildInfo;
pub const STORE_SQLITE: BuildInfo =
    BuildInfo::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
