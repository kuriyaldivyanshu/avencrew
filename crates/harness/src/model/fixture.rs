//! Pure, bounded normalization of finite synthetic provider streams.
//! No result is released until the whole stream has been inspected.
use avencrew_contracts::{
    canonical_bytes, decode_internal_message, encode_message,
    scalars::{Counter, Digest, DomainId, List, Nullable, Text},
    wire::{
        ErrorCode, NormalizedModelResponse, NormalizedModelResponseFinish as Finish,
        NormalizedModelResponseItemsItem as Item, ProtocolVersion, SafeError, WireMessage,
    },
    Validate,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{self, Write},
};

const MAX_INPUT: usize = 1_048_576;
const MAX_ITEM_BYTES: usize = 262_144;
const MAX_EVENTS: usize = 4096;
const MAX_ITEMS: usize = 100;
const VERSION: &str = "avencrew.fixture-model/1";

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FixtureInput {
    pub schema_version: String,
    pub seed: FixtureSeed,
    pub events: Vec<FixtureEvent>,
}
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FixtureSeed {
    pub model_call_id: DomainId,
    pub step_id: DomainId,
    pub provider_profile_id: DomainId,
    pub compatibility_digest: Digest,
    pub bindings: Vec<FixtureBinding>,
    pub invocation_slots: Vec<DomainId>,
    pub continuation_refs: Vec<DomainId>,
    pub usage_ref: DomainId,
}
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FixtureBinding {
    pub name: String,
    pub binding_id: DomainId,
}
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FixtureEvent {
    pub sequence: Counter,
    pub body: FixtureEventBody,
}
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FixtureEventBody {
    TextStart {
        item: u32,
    },
    TextDelta {
        item: u32,
        bytes: Vec<u8>,
    },
    TextEnd {
        item: u32,
    },
    ToolStart {
        item: u32,
        provider_call_id: String,
        tool_name: String,
    },
    ArgumentsDelta {
        item: u32,
        bytes: Vec<u8>,
    },
    ToolEnd {
        item: u32,
    },
    Opaque {
        item: u32,
        provider_profile_id: DomainId,
        compatibility_digest: Digest,
        blob_ref: DomainId,
    },
    Usage {
        input_tokens: Counter,
        output_tokens: Counter,
    },
    Finish {
        reason: Finish,
    },
}
pub use avencrew_contracts::runtime::{
    FixtureError, FixtureFailure, FixtureResult, FixtureUsage, ProposalIdentity,
};

fn failure(code: FixtureError, usage: &FixtureUsage) -> FixtureFailure {
    FixtureFailure {
        code,
        usage: usage.clone(),
    }
}

// Measure direct inputs without allocating a serialized copy.
struct CountBytes(usize);
impl Write for CountBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= MAX_INPUT)
            .ok_or_else(|| io::Error::other("fixture input limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn binding_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}
fn unique<T: Ord>(values: impl IntoIterator<Item = T>) -> bool {
    let mut seen = BTreeSet::new();
    values.into_iter().all(|value| seen.insert(value))
}
fn validate_input(input: &FixtureInput) -> Result<(), FixtureError> {
    use FixtureError::*;
    if input.schema_version != VERSION {
        return Err(IncompatibleVersion);
    }
    let seed = &input.seed;
    if input.events.len() > MAX_EVENTS
        || seed.bindings.len() > MAX_ITEMS
        || seed.invocation_slots.len() > MAX_ITEMS
        || seed.continuation_refs.len() > MAX_ITEMS
    {
        return Err(LimitExceeded);
    }
    if !seed.bindings.iter().all(|b| binding_name(&b.name))
        || !unique(seed.bindings.iter().map(|b| &b.name))
        || !unique(seed.bindings.iter().map(|b| &b.binding_id))
        || !unique(&seed.invocation_slots)
        || !unique(&seed.continuation_refs)
        || seed.invocation_slots.iter().any(|id| {
            [
                &seed.model_call_id,
                &seed.step_id,
                &seed.provider_profile_id,
                &seed.usage_ref,
            ]
            .contains(&id)
        })
    {
        return Err(InvalidInput);
    }
    let mut delta_bytes = 0usize;
    for event in &input.events {
        if let FixtureEventBody::TextDelta { bytes, .. }
        | FixtureEventBody::ArgumentsDelta { bytes, .. } = &event.body
        {
            delta_bytes = delta_bytes
                .checked_add(bytes.len())
                .filter(|n| *n <= MAX_INPUT)
                .ok_or(LimitExceeded)?;
        }
    }
    serde_json::to_writer(CountBytes(0), input).map_err(|_| LimitExceeded)?;
    // DomainId, Digest and Counter validate on construction/deserialization;
    // their private representations cannot be changed by direct callers.
    Ok(())
}

/// Decode a strict, bounded fixture input. Decode failures have unknown usage;
/// event effects are observed only by normalization.
pub fn decode_fixture(bytes: &[u8]) -> Result<FixtureInput, FixtureFailure> {
    let unknown = FixtureUsage::Unknown;
    if bytes.len() > MAX_INPUT {
        return Err(failure(FixtureError::LimitExceeded, &unknown));
    }
    let canonical =
        canonical_bytes(bytes).map_err(|_| failure(FixtureError::InvalidInput, &unknown))?;
    let input = serde_json::from_slice(&canonical)
        .map_err(|_| failure(FixtureError::InvalidInput, &unknown))?;
    validate_input(&input).map_err(|code| failure(code, &unknown))?;
    Ok(input)
}

enum PendingItem {
    Text(Vec<u8>),
    Tool {
        bytes: Vec<u8>,
        provider_call_id: String,
        binding_id: DomainId,
    },
    Closed(Item),
}
fn allocate(
    items: &mut Vec<PendingItem>,
    index: u32,
    item: PendingItem,
) -> Result<(), FixtureError> {
    if index as usize >= MAX_ITEMS {
        return Err(FixtureError::LimitExceeded);
    }
    if index as usize != items.len() {
        return Err(FixtureError::InvalidItemState);
    }
    items.push(item);
    Ok(())
}
fn append(buffer: &mut Vec<u8>, bytes: &[u8]) -> Result<(), FixtureError> {
    buffer
        .len()
        .checked_add(bytes.len())
        .filter(|n| *n <= MAX_ITEM_BYTES)
        .ok_or(FixtureError::LimitExceeded)?;
    buffer.extend_from_slice(bytes);
    Ok(())
}
fn tool_item(
    seed: &FixtureSeed,
    call: &str,
    binding: &DomainId,
    bytes: &[u8],
) -> Result<Item, FixtureError> {
    let arguments = std::str::from_utf8(bytes).map_err(|_| FixtureError::InvalidUtf8)?;
    let call = serde_json::to_string(call).map_err(|_| FixtureError::InvalidArguments)?;
    // Insert raw arguments into a bounded envelope BEFORE any Value parse so
    // the contracts decoder can detect duplicate (including escaped) keys.
    let envelope = format!(
        r#"{{"schema_version":"1.0","message_kind":"normalized_model_response","payload":{{"model_call_id":"{}","step_id":"{}","items":[{{"kind":"tool_proposal","provider_call_id":{},"binding_id":"{}","arguments":{}}}],"finish":"tools","usage_refs":[],"diagnostic_error":null}}}}"#,
        seed.model_call_id.as_str(),
        seed.step_id.as_str(),
        call,
        binding.as_str(),
        arguments
    );
    let WireMessage::NormalizedModelResponse { payload, .. } =
        decode_internal_message(envelope.as_bytes()).map_err(|_| FixtureError::InvalidArguments)?
    else {
        return Err(FixtureError::InvalidArguments);
    };
    // Arguments must occupy exactly one JSON object value. A fragment that
    // escapes its value into another envelope item is trailing input too.
    if payload.items.as_slice().len() != 1 {
        return Err(FixtureError::InvalidArguments);
    }
    payload
        .items
        .as_slice()
        .first()
        .cloned()
        .ok_or(FixtureError::InvalidArguments)
}

/// Normalize a whole finite stream without dispatch or persistence. Returned
/// proposals require host commitment and current-policy authorization.
/// Failures retain the first valid usage measurement already consumed.
pub fn normalize_fixture(input: &FixtureInput) -> Result<FixtureResult, FixtureFailure> {
    let mut usage = FixtureUsage::Unknown;
    validate_input(input).map_err(|code| failure(code, &usage))?;
    let seed = &input.seed;
    let mut items = Vec::new();
    let mut identities = Vec::new();
    let mut calls = BTreeSet::new();
    let mut consumed: Vec<&FixtureEvent> = Vec::new();
    let mut finish = None;
    for event in &input.events {
        let sequence = event.sequence.value();
        if sequence == 0 {
            return Err(failure(FixtureError::InvalidSequence, &usage));
        }
        if sequence <= consumed.len() as u64 {
            if consumed[(sequence - 1) as usize] != event {
                return Err(failure(FixtureError::ReplayMismatch, &usage));
            }
            continue;
        }
        if finish.is_some() {
            return Err(failure(FixtureError::InvalidFinish, &usage));
        }
        if sequence != consumed.len() as u64 + 1 {
            return Err(failure(FixtureError::InvalidSequence, &usage));
        }
        let processed = (|| -> Result<(), FixtureError> {
            use FixtureError::*;
            use FixtureEventBody::*;
            match &event.body {
                TextStart { item } => allocate(&mut items, *item, PendingItem::Text(Vec::new()))?,
                TextDelta { item, bytes } => {
                    let Some(PendingItem::Text(buffer)) = items.get_mut(*item as usize) else {
                        return Err(InvalidItemState);
                    };
                    append(buffer, bytes)?;
                }
                TextEnd { item } => {
                    let Some(pending @ PendingItem::Text(_)) = items.get_mut(*item as usize) else {
                        return Err(InvalidItemState);
                    };
                    let PendingItem::Text(bytes) = pending else {
                        unreachable!()
                    };
                    let text = std::str::from_utf8(bytes).map_err(|_| InvalidUtf8)?;
                    *pending = PendingItem::Closed(Item::Text {
                        text: Text::new(text).map_err(|_| LimitExceeded)?,
                    });
                }
                ToolStart {
                    item,
                    provider_call_id,
                    tool_name,
                } => {
                    if provider_call_id.is_empty()
                        || provider_call_id.len() > 256
                        || !binding_name(tool_name)
                    {
                        return Err(InvalidInput);
                    }
                    if !calls.insert(provider_call_id.as_str()) {
                        return Err(DuplicateCall);
                    }
                    let binding = seed
                        .bindings
                        .iter()
                        .find(|b| b.name == *tool_name)
                        .ok_or(UnknownBinding)?;
                    let invocation_id = seed
                        .invocation_slots
                        .get(identities.len())
                        .ok_or(LimitExceeded)?
                        .clone();
                    allocate(
                        &mut items,
                        *item,
                        PendingItem::Tool {
                            bytes: Vec::new(),
                            provider_call_id: provider_call_id.clone(),
                            binding_id: binding.binding_id.clone(),
                        },
                    )?;
                    identities.push(ProposalIdentity {
                        item: *item,
                        provider_call_id: provider_call_id.clone(),
                        invocation_id,
                    });
                }
                ArgumentsDelta { item, bytes } => {
                    let Some(PendingItem::Tool { bytes: buffer, .. }) =
                        items.get_mut(*item as usize)
                    else {
                        return Err(InvalidItemState);
                    };
                    append(buffer, bytes)?;
                }
                ToolEnd { item } => {
                    let Some(pending @ PendingItem::Tool { .. }) = items.get_mut(*item as usize)
                    else {
                        return Err(InvalidItemState);
                    };
                    let PendingItem::Tool {
                        bytes,
                        provider_call_id,
                        binding_id,
                    } = pending
                    else {
                        unreachable!()
                    };
                    *pending =
                        PendingItem::Closed(tool_item(seed, provider_call_id, binding_id, bytes)?);
                }
                Opaque {
                    item,
                    provider_profile_id,
                    compatibility_digest,
                    blob_ref,
                } => {
                    if *provider_profile_id != seed.provider_profile_id
                        || *compatibility_digest != seed.compatibility_digest
                        || !seed.continuation_refs.contains(blob_ref)
                    {
                        return Err(IncompatibleContinuation);
                    }
                    allocate(
                        &mut items,
                        *item,
                        PendingItem::Closed(Item::OpaqueContinuation {
                            provider_profile_id: provider_profile_id.clone(),
                            compatibility_digest: compatibility_digest.clone(),
                            blob_ref: blob_ref.clone(),
                        }),
                    )?;
                }
                Usage {
                    input_tokens,
                    output_tokens,
                } => {
                    if usage != FixtureUsage::Unknown {
                        return Err(InvalidItemState);
                    }
                    usage = FixtureUsage::Known {
                        input_tokens: input_tokens.clone(),
                        output_tokens: output_tokens.clone(),
                    };
                }
                Finish { reason } => {
                    finish = Some(reason.clone());
                }
            }
            Ok(())
        })();
        processed.map_err(|code| failure(code, &usage))?;
        consumed.push(event);
    }
    let finish = finish.unwrap_or(Finish::Interrupted);
    let complete = matches!(finish, Finish::Tools | Finish::Final | Finish::Refused);
    if complete
        && (items
            .iter()
            .any(|item| !matches!(item, PendingItem::Closed(_)))
            || (matches!(finish, Finish::Tools) && identities.is_empty())
            || (!matches!(finish, Finish::Tools) && !identities.is_empty()))
    {
        return Err(failure(FixtureError::InvalidFinish, &usage));
    }
    if !complete {
        identities.clear();
    }
    let items = items
        .into_iter()
        .filter_map(|item| match item {
            PendingItem::Closed(item) if complete || matches!(item, Item::Text { .. }) => {
                Some(item)
            }
            _ => None,
        })
        .collect();
    let result = (|| {
        let response = NormalizedModelResponse {
            model_call_id: seed.model_call_id.clone(),
            step_id: seed.step_id.clone(),
            items: List::new(items)?,
            finish: finish.clone(),
            usage_refs: List::new(if usage == FixtureUsage::Unknown {
                vec![]
            } else {
                vec![seed.usage_ref.clone()]
            })?,
            diagnostic_error: Nullable(if finish == Finish::Failed {
                Some(SafeError {
                    code: ErrorCode::Unavailable,
                    message: Text::new("Fixture model stream failed")?,
                    evidence_refs: List::new(vec![])?,
                })
            } else {
                None
            }),
        };
        response.validate()?;
        encode_message(&WireMessage::NormalizedModelResponse {
            schema_version: ProtocolVersion::V10,
            payload: Box::new(response.clone()),
        })?;
        Ok::<_, avencrew_contracts::ContractError>(response)
    })()
    .map_err(|_| failure(FixtureError::InvalidResponse, &usage))?;
    Ok(FixtureResult {
        response: result,
        usage,
        proposal_identities: identities,
    })
}

/// Stage a complete fixture response through the same broker used by other
/// origins. Every mapping is checked before the host sees any proposals.
/// Usage persistence and model-response journaling remain coordinator-owned.
pub async fn commit_fixture_proposals<H: avencrew_broker::Host>(
    broker: &avencrew_broker::Broker,
    host: &mut H,
    expected_model_call: &DomainId,
    result: &FixtureResult,
) -> Result<Vec<avencrew_broker::PreparedInvocation>, avencrew_broker::BrokerError> {
    use avencrew_broker::{BrokerError, Invocation};
    if result.response.model_call_id != *expected_model_call
        || result.response.step_id != broker.step().step_id
        || result.response.finish != Finish::Tools
    {
        return Err(BrokerError::InvalidArguments);
    }
    let mut proposals = Vec::new();
    for (index, item) in result.response.items.as_slice().iter().enumerate() {
        if let Item::ToolProposal {
            provider_call_id, ..
        } = item
        {
            let identity = result
                .proposal_identities
                .get(proposals.len())
                .ok_or(BrokerError::InvalidArguments)?;
            if identity.item as usize != index
                || identity.provider_call_id != provider_call_id.as_str()
            {
                return Err(BrokerError::InvalidArguments);
            }
            proposals.push(Invocation::from_model(
                identity.invocation_id.clone(),
                expected_model_call.clone(),
                item,
            )?);
        }
    }
    if proposals.is_empty() || proposals.len() != result.proposal_identities.len() {
        return Err(BrokerError::InvalidArguments);
    }
    // Direct Rust callers can construct a FixtureResult too. Check all proposal
    // buffers before traversing/encoding the whole response, then enforce the
    // same normalized wire bound used by the adapter.
    serde_json::to_writer(CountBytes(0), &result.response)
        .map_err(|_| BrokerError::LimitExceeded)?;
    encode_message(&WireMessage::NormalizedModelResponse {
        schema_version: ProtocolVersion::V10,
        payload: Box::new(result.response.clone()),
    })
    .map_err(|_| BrokerError::InvalidArguments)?;
    broker.commit(host, proposals).await
}
