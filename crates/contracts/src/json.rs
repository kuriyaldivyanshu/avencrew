use crate::{
    wire::{RpcRequest, WireMessage},
    ContractError, Validate,
};
use serde::{
    de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor},
    Deserialize,
};
use serde_json::Value;
use std::fmt;

pub const MAX_FRAME_BYTES: usize = 1_048_576;

// A Value parse normally silently overwrites repeated keys. This visitor runs
// before Serde's tagged/untagged buffering, including for opaque tool arguments.
struct StrictJson;
impl<'de> DeserializeSeed<'de> for StrictJson {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, de: D) -> Result<Value, D::Error> {
        de.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for StrictJson {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON without duplicate keys")
    }
    fn visit_bool<E: Error>(self, v: bool) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_i64<E: Error>(self, v: i64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: Error>(self, v: u64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite number"))
    }
    fn visit_str<E: Error>(self, v: &str) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_string<E: Error>(self, v: String) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_unit<E: Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(v) = seq.next_element_seed(StrictJson)? {
            values.push(v);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(A::Error::custom("duplicate object key"));
            }
            values.insert(key, map.next_value_seed(StrictJson)?);
        }
        Ok(Value::Object(values))
    }
}
fn parse(bytes: &[u8]) -> Result<Value, ContractError> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(ContractError::Limit("frame bytes"));
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value = StrictJson
        .deserialize(&mut de)
        .map_err(|_| ContractError::InvalidMessage)?;
    de.end().map_err(|_| ContractError::InvalidMessage)?;
    Ok(value)
}

/// Decode a full internal envelope. The caller must already authenticate the
/// protected peer; a decoded lease/context is still not an authority grant.
pub fn decode_internal_message(bytes: &[u8]) -> Result<WireMessage, ContractError> {
    let value = parse(bytes)?;
    match value.get("schema_version") {
        Some(Value::String(v)) if v == "1.0" => {}
        Some(Value::String(_)) => return Err(ContractError::IncompatibleVersion),
        _ => return Err(ContractError::InvalidMessage),
    }
    let message = WireMessage::deserialize(value).map_err(|_| ContractError::InvalidMessage)?;
    message.validate()?;
    Ok(message)
}

/// Parse only the external client operation surface. Actor/workspace authority,
/// epochs, budget holds and leases are absent from these strict request bodies.
/// Session authentication and live authorization belong to the coordinator.
pub fn decode_client_request(bytes: &[u8]) -> Result<RpcRequest, ContractError> {
    let WireMessage::RpcRequest { payload, .. } = decode_internal_message(bytes)? else {
        return Err(ContractError::UntrustedOperation);
    };
    match *payload {
        RpcRequest::Handshake { .. }
        | RpcRequest::DescribeCapabilities { .. }
        | RpcRequest::SubmitTask { .. }
        | RpcRequest::SubmitCommand { .. }
        | RpcRequest::GetTaskSnapshot { .. }
        | RpcRequest::Subscribe { .. }
        | RpcRequest::AnswerClarification { .. }
        | RpcRequest::ReviewArtifact { .. }
        | RpcRequest::DecideAction { .. }
        | RpcRequest::GetArtifact { .. }
        | RpcRequest::PrepareTransfer { .. }
        | RpcRequest::QueryTransfer { .. }
        | RpcRequest::RequestTransferAbort { .. }
        | RpcRequest::GetUsage { .. } => Ok(*payload),
        _ => Err(ContractError::UntrustedOperation),
    }
}

/// Validate before serializing constructed DTOs; output is still frame-bounded.
pub fn encode_message(message: &WireMessage) -> Result<Vec<u8>, ContractError> {
    message.validate()?;
    bounded_json(message, MAX_FRAME_BYTES, "frame bytes")
}

/// Foundation canonical JSON bytes, suitable as input to SHA-256. Reject floats
/// (including 1.0/exponent notation) and duplicates before they can be normalized.
/// UTF-8 object key ordering equals Unicode scalar ordering; strings retain their
/// exact scalar contents, including normalization form.
pub fn canonical_bytes(bytes: &[u8]) -> Result<Vec<u8>, ContractError> {
    fn order(value: Value) -> Result<Value, ContractError> {
        match value {
            Value::Number(ref n) if !n.is_i64() && !n.is_u64() => {
                Err(ContractError::NonIntegralCanonicalNumber)
            }
            Value::Array(values) => values
                .into_iter()
                .map(order)
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array),
            Value::Object(values) => {
                let mut entries: Vec<_> = values.into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                let mut map = serde_json::Map::new();
                for (key, value) in entries {
                    map.insert(key, order(value)?);
                }
                Ok(Value::Object(map))
            }
            other => Ok(other),
        }
    }
    serde_json::to_vec(&order(parse(bytes)?)?).map_err(|_| ContractError::InvalidMessage)
}

// Cap allocations during encoding, including opaque tool data constructed by
// trusted consumers. Rejecting only after `to_vec` would allocate past the cap.
pub(crate) fn bounded_json<T: serde::Serialize + ?Sized>(
    value: &T,
    limit: usize,
    label: &'static str,
) -> Result<Vec<u8>, ContractError> {
    struct Output {
        bytes: Vec<u8>,
        limit: usize,
        exceeded: bool,
    }
    impl std::io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit - self.bytes.len() {
                self.exceeded = true;
                return Err(std::io::Error::other("JSON byte limit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Output {
        bytes: Vec::new(),
        limit,
        exceeded: false,
    };
    if serde_json::to_writer(&mut output, value).is_err() {
        return Err(if output.exceeded {
            ContractError::Limit(label)
        } else {
            ContractError::InvalidMessage
        });
    }
    Ok(output.bytes)
}
