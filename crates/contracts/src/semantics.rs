//! Portable consistency rules. Current grants, journal revisions and lease
//! ownership must still be checked by the authenticated execution authority.
use crate::{wire::*, ContractError};

fn paired(environment: bool, generation: bool) -> Result<(), ContractError> {
    if environment != generation {
        return Err(ContractError::InvalidUnion("environment/generation"));
    }
    Ok(())
}
pub fn lease(value: &Lease) -> Result<(), ContractError> {
    paired(
        value.environment_id.0.is_some(),
        value.generation.0.is_some(),
    )?;
    if value.authority == ExecutionAuthority::Local && value.placement == Placement::Cloud {
        return Err(ContractError::InvalidUnion(
            "local authority/cloud placement",
        ));
    }
    Ok(())
}
pub fn trusted_context(value: &TrustedContext) -> Result<(), ContractError> {
    paired(
        value.environment_id.0.is_some(),
        value.generation.0.is_some(),
    )
}
pub fn checkpoint(value: &Checkpoint) -> Result<(), ContractError> {
    if value.applied_seq.value() > value.accepted_seq.value() {
        return Err(ContractError::InvalidUnion("applied/accepted cursor"));
    }
    Ok(())
}
pub fn snapshot(value: &TaskSnapshot) -> Result<(), ContractError> {
    if value.applied_seq.value() > value.accepted_seq.value() {
        return Err(ContractError::InvalidUnion("applied/accepted cursor"));
    }
    if value.authority == Authority::Local && value.placement == Placement::Cloud {
        return Err(ContractError::InvalidUnion(
            "local authority/cloud placement",
        ));
    }
    Ok(())
}
pub fn transfer(value: &TransferReceipt) -> Result<(), ContractError> {
    if value.disposition == TransferDisposition::Committed {
        let expected = value.source_epoch.value().checked_add(1);
        if value.authority != Authority::Server
            || value.receipt_ref.0.is_none()
            || value.destination_epoch.0.as_ref().map(|v| v.value()) != expected
        {
            return Err(ContractError::InvalidUnion("committed transfer"));
        }
    }
    Ok(())
}
