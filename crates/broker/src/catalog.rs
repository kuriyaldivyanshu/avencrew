use crate::BrokerError;
use avencrew_contracts::{
    canonical_bytes,
    scalars::{Digest, DomainId},
};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeSet, sync::Arc};

const MAX_BINDINGS: usize = 100;
const MAX_SCHEMA_BYTES: usize = 32_768;
const MAX_CATALOG_BYTES: usize = 1_048_576;

pub use avencrew_contracts::runtime::{Binding, Effect};

#[derive(Clone)]
pub struct SealedCatalog {
    bindings: Vec<Binding>,
    validators: Vec<Arc<jsonschema::Validator>>,
    output_validators: Vec<Arc<jsonschema::Validator>>,
    digest: Digest,
}

/// A cursor is a numeric offset into this immutable catalog, not a live registry.
pub struct CatalogPage<'a> {
    pub bindings: Vec<&'a Binding>,
    pub next_offset: Option<usize>,
}

pub(crate) fn hash(bytes: &[u8]) -> Digest {
    let hex: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Digest::new(hex).expect("SHA-256 hex is a valid digest")
}
pub(crate) fn name_valid(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

// Initial profile is self-contained, deliberately without schema references,
// regexes, custom formats or embedded vocabularies. No retriever/network is used.
// The allowlist applies at schema positions, not to user property names.
fn schema_profile(schema: &Value, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    let Some(object) = schema.as_object() else {
        return schema.is_boolean();
    };
    object.iter().all(|(key, value)| match key.as_str() {
        "type" => value.is_string() || value.is_array(),
        "title" | "description" => value.is_string(),
        "enum" => value.is_array(),
        "const" => true,
        "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" | "minLength"
        | "maxLength" | "minItems" | "maxItems" | "minProperties" | "maxProperties" => {
            value.is_u64() || value.is_i64()
        }
        "required" => value
            .as_array()
            .is_some_and(|v| v.iter().all(Value::is_string)),
        "additionalProperties" => value == &Value::Bool(false),
        "properties" => value
            .as_object()
            .is_some_and(|properties| properties.values().all(|s| schema_profile(s, depth + 1))),
        "items" => schema_profile(value, depth + 1),
        _ => false,
    })
}

impl SealedCatalog {
    pub fn seal(mut bindings: Vec<Binding>) -> Result<Self, BrokerError> {
        if bindings.len() > MAX_BINDINGS {
            return Err(BrokerError::LimitExceeded);
        }
        bindings.sort_by(|a, b| a.binding_id.cmp(&b.binding_id));
        let mut names = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut validators = Vec::new();
        let mut output_validators = Vec::new();
        for binding in &bindings {
            if !name_valid(&binding.name)
                || !names.insert(&binding.name)
                || !ids.insert(&binding.binding_id)
            {
                return Err(BrokerError::InvalidCatalog);
            }
            crate::bounds::depth(&binding.input_schema)?;
            crate::bounds::encode(&binding.input_schema, MAX_SCHEMA_BYTES)?;
            if binding.input_schema["type"] != "object"
                || binding.input_schema["additionalProperties"] != false
                || !schema_profile(&binding.input_schema, 0)
            {
                return Err(BrokerError::InvalidCatalog);
            }
            validators.push(Arc::new(
                jsonschema::options()
                    .with_draft(jsonschema::Draft::Draft202012)
                    .should_validate_formats(false)
                    .build(&binding.input_schema)
                    .map_err(|_| BrokerError::InvalidCatalog)?,
            ));
            crate::bounds::depth(&binding.output_schema)?;
            crate::bounds::encode(&binding.output_schema, MAX_SCHEMA_BYTES)?;
            if !schema_profile(&binding.output_schema, 0) {
                return Err(BrokerError::InvalidCatalog);
            }
            output_validators.push(Arc::new(
                jsonschema::options()
                    .with_draft(jsonschema::Draft::Draft202012)
                    .should_validate_formats(false)
                    .build(&binding.output_schema)
                    .map_err(|_| BrokerError::InvalidCatalog)?,
            ));
        }
        let bytes = crate::bounds::encode(&bindings, MAX_CATALOG_BYTES)?;
        let canonical = canonical_bytes(&bytes).map_err(|_| BrokerError::InvalidCatalog)?;
        // Version the recipe, including every descriptor and its entire schema.
        let mut digest_bytes = b"avencrew.sealed-catalog/1\0".to_vec();
        digest_bytes.extend(canonical);
        Ok(Self {
            bindings,
            validators,
            output_validators,
            digest: hash(&digest_bytes),
        })
    }
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }
    pub fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<CatalogPage<'_>, BrokerError> {
        if query.len() > 256 || !(1..=100).contains(&limit) || offset > self.bindings.len() {
            return Err(BrokerError::LimitExceeded);
        }
        let mut matches = self
            .bindings
            .iter()
            .enumerate()
            .skip(offset)
            .filter(|(_, b)| b.name.contains(query));
        let mut bindings = Vec::new();
        for _ in 0..limit {
            let Some((_, binding)) = matches.next() else {
                break;
            };
            bindings.push(binding);
        }
        let next_offset = matches.next().map(|(index, _)| index);
        Ok(CatalogPage {
            bindings,
            next_offset,
        })
    }
    pub(crate) fn validate(
        &self,
        id: &DomainId,
        arguments: &Value,
    ) -> Result<&Binding, BrokerError> {
        let index = self
            .bindings
            .iter()
            .position(|b| &b.binding_id == id)
            .ok_or(BrokerError::UnsealedBinding)?;
        if !self.validators[index].is_valid(arguments) {
            return Err(BrokerError::InvalidArguments);
        }
        Ok(&self.bindings[index])
    }
    pub(crate) fn validate_output(&self, id: &DomainId, value: &Value) -> bool {
        self.bindings
            .iter()
            .position(|b| &b.binding_id == id)
            .is_some_and(|i| self.output_validators[i].is_valid(value))
    }
}
