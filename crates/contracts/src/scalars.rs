//! Precision-preserving scalars and bounds shared by every wire message.
use crate::{ContractError, Validate};
use serde::{de::Error, Deserialize, Deserializer, Serialize};

/// Validation rules used by both constructors and deserialization.
trait StringRule {
    fn check(value: &str) -> bool;
}

macro_rules! string_scalar {
    ($name:ident, $pattern:literal, $check:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl StringRule for $name {
            fn check(value: &str) -> bool { ($check)(value) }
        }
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ContractError> {
                let value = value.into();
                if !Self::check(&value) { return Err(ContractError::InvalidScalar(stringify!($name))); }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str { &self.0 }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
                Self::new(String::deserialize(de)?).map_err(D::Error::custom)
            }
        }
        impl Validate for $name {
            fn validate(&self) -> Result<(), ContractError> { Ok(()) }
        }
        #[cfg(feature = "schema-export")]
        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> { stringify!($name).into() }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({"type":"string", "pattern": $pattern})
            }
        }
        #[cfg(feature = "typescript-export")]
        impl ts_rs::TS for $name {
            type WithoutGenerics = Self;
            type OptionInnerType = Self;
            fn name(_: &ts_rs::Config) -> String { "string".into() }
            fn inline(cfg: &ts_rs::Config) -> String { Self::name(cfg) }
        }
    };
}

fn canonical_counter(value: &str) -> bool {
    !value.is_empty()
        && (value == "0" || !value.starts_with('0'))
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.parse::<i64>().is_ok()
}

string_scalar!(Counter, "^(0|[1-9][0-9]{0,18})$", canonical_counter);
string_scalar!(PositiveCounter, "^[1-9][0-9]{0,18}$", |s: &str| {
    s != "0" && canonical_counter(s)
});
impl Counter {
    pub fn value(&self) -> u64 {
        self.0.parse().expect("validated counter")
    }
}
impl PositiveCounter {
    pub fn value(&self) -> u64 {
        self.0.parse().expect("validated positive counter")
    }
}

string_scalar!(
    DomainId,
    "^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    |s: &str| {
        s.len() == 36
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-')
            && uuid::Uuid::parse_str(s).is_ok_and(|id| {
                id.get_version_num() == 7
                    && id.get_variant() == uuid::Variant::RFC4122
                    && id.hyphenated().to_string() == s
            })
    }
);
string_scalar!(Digest, "^[0-9a-f]{64}$", |s: &str| {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
});
string_scalar!(
    Instant,
    "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\\.[0-9]{6}Z$",
    |s: &str| {
        s.len() == 27
            && s.is_ascii()
            && s.as_bytes()[26] == b'Z'
            && chrono::DateTime::parse_from_rfc3339(s).is_ok_and(|t| {
                // Reject leap-second representations; the journal stores UTC microseconds.
                t.timestamp_subsec_nanos() < 1_000_000_000
                    && t.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string() == s
            })
    }
);
impl Instant {
    pub fn unix_micros(&self) -> i64 {
        chrono::DateTime::parse_from_rfc3339(&self.0)
            .expect("validated instant")
            .timestamp_micros()
    }
}
string_scalar!(Money, "^-?(0|[1-9][0-9]*)\\.[0-9]{6}$", |s: &str| {
    let s = s.strip_prefix('-').unwrap_or(s);
    s.split_once('.').is_some_and(|(whole, fraction)| {
        !whole.is_empty()
            && (whole == "0" || !whole.starts_with('0'))
            && whole.bytes().all(|b| b.is_ascii_digit())
            && fraction.len() == 6
            && fraction.bytes().all(|b| b.is_ascii_digit())
    })
});

/// JSON Schema length counts Unicode scalar values, not UTF-8 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Text<const MIN: usize, const MAX: usize>(String);
impl<const MIN: usize, const MAX: usize> Text<MIN, MAX> {
    pub fn new(value: impl Into<String>) -> Result<Self, ContractError> {
        let value = value.into();
        let count = value.chars().count();
        if !(MIN..=MAX).contains(&count) {
            return Err(ContractError::Limit("text length"));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de, const MIN: usize, const MAX: usize> Deserialize<'de> for Text<MIN, MAX> {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(de)?).map_err(D::Error::custom)
    }
}
impl<const MIN: usize, const MAX: usize> Validate for Text<MIN, MAX> {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct List<T, const MIN: usize, const MAX: usize>(Vec<T>);
impl<T, const MIN: usize, const MAX: usize> List<T, MIN, MAX> {
    pub fn new(value: Vec<T>) -> Result<Self, ContractError> {
        if !(MIN..=MAX).contains(&value.len()) {
            return Err(ContractError::Limit("list length"));
        }
        Ok(Self(value))
    }
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }
}
impl<'de, T: Deserialize<'de>, const MIN: usize, const MAX: usize> Deserialize<'de>
    for List<T, MIN, MAX>
{
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        Self::new(Vec::<T>::deserialize(de)?).map_err(D::Error::custom)
    }
}
impl<T: Validate, const MIN: usize, const MAX: usize> Validate for List<T, MIN, MAX> {
    fn validate(&self) -> Result<(), ContractError> {
        for value in &self.0 {
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SmallInteger<const MIN: u64, const MAX: u64>(u64);
impl<const MIN: u64, const MAX: u64> SmallInteger<MIN, MAX> {
    pub fn new(value: u64) -> Result<Self, ContractError> {
        if !(MIN..=MAX).contains(&value) {
            return Err(ContractError::Limit("integer range"));
        }
        Ok(Self(value))
    }
    pub fn value(&self) -> u64 {
        self.0
    }
}
impl<'de, const MIN: u64, const MAX: u64> Deserialize<'de> for SmallInteger<MIN, MAX> {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        Self::new(u64::deserialize(de)?).map_err(D::Error::custom)
    }
}
impl<const MIN: u64, const MAX: u64> Validate for SmallInteger<MIN, MAX> {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

/// The key is mandatory; null is an explicit value, never an omitted field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Nullable<T>(pub Option<T>);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Nullable<T> {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(de)?;
        if value.is_null() {
            Ok(Self(None))
        } else {
            T::deserialize(value)
                .map(|v| Self(Some(v)))
                .map_err(D::Error::custom)
        }
    }
}
impl<T: Validate> Validate for Nullable<T> {
    fn validate(&self) -> Result<(), ContractError> {
        if let Some(value) = &self.0 {
            value.validate()?;
        }
        Ok(())
    }
}

/// Open tool arguments remain data. The broker must validate the sealed binding
/// schema before accepting a proposal; this wrapper does not confer authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct JsonObject<const MAX: usize>(serde_json::Map<String, serde_json::Value>);
impl<const MAX: usize> JsonObject<MAX> {
    pub fn new(value: serde_json::Map<String, serde_json::Value>) -> Result<Self, ContractError> {
        if value.len() > MAX {
            return Err(ContractError::Limit("object properties"));
        }
        Ok(Self(value))
    }
    pub fn as_map(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.0
    }
}
impl<'de, const MAX: usize> Deserialize<'de> for JsonObject<MAX> {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        Self::new(serde_json::Map::deserialize(de)?).map_err(D::Error::custom)
    }
}
impl<const MAX: usize> Validate for JsonObject<MAX> {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[cfg(feature = "schema-export")]
mod schema {
    use super::*;
    use schemars::{JsonSchema, Schema, SchemaGenerator};
    use std::borrow::Cow;
    impl<const MIN: usize, const MAX: usize> JsonSchema for Text<MIN, MAX> {
        fn schema_name() -> Cow<'static, str> {
            format!("Text_{MIN}_{MAX}").into()
        }
        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            schemars::json_schema!({"type":"string","minLength":MIN,"maxLength":MAX})
        }
    }
    impl<T: JsonSchema, const MIN: usize, const MAX: usize> JsonSchema for List<T, MIN, MAX> {
        fn schema_name() -> Cow<'static, str> {
            format!("List_{}_{MIN}_{MAX}", T::schema_name()).into()
        }
        fn json_schema(generator: &mut SchemaGenerator) -> Schema {
            schemars::json_schema!({"type":"array","items":generator.subschema_for::<T>(),"minItems":MIN,"maxItems":MAX})
        }
    }
    impl<const MIN: u64, const MAX: u64> JsonSchema for SmallInteger<MIN, MAX> {
        fn schema_name() -> Cow<'static, str> {
            format!("Integer_{MIN}_{MAX}").into()
        }
        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            schemars::json_schema!({"type":"integer","minimum":MIN,"maximum":MAX})
        }
    }
    impl<T: JsonSchema> JsonSchema for Nullable<T> {
        fn schema_name() -> Cow<'static, str> {
            format!("Nullable_{}", T::schema_name()).into()
        }
        fn json_schema(generator: &mut SchemaGenerator) -> Schema {
            <Option<T>>::json_schema(generator)
        }
    }
    impl<const MAX: usize> JsonSchema for JsonObject<MAX> {
        fn schema_name() -> Cow<'static, str> {
            format!("Object_{MAX}").into()
        }
        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            schemars::json_schema!({"type":"object","maxProperties":MAX})
        }
    }
}

#[cfg(feature = "typescript-export")]
mod typescript {
    use super::*;
    use ts_rs::{Config, TypeVisitor, TS};
    impl<const MIN: usize, const MAX: usize> TS for Text<MIN, MAX> {
        type WithoutGenerics = Self;
        type OptionInnerType = Self;
        fn name(_: &Config) -> String {
            "string".into()
        }
        fn inline(cfg: &Config) -> String {
            Self::name(cfg)
        }
    }
    impl<const MIN: u64, const MAX: u64> TS for SmallInteger<MIN, MAX> {
        type WithoutGenerics = Self;
        type OptionInnerType = Self;
        fn name(_: &Config) -> String {
            if MIN == MAX {
                MIN.to_string()
            } else {
                "number".into()
            }
        }
        fn inline(cfg: &Config) -> String {
            Self::name(cfg)
        }
    }
    impl<T: TS, const MIN: usize, const MAX: usize> TS for List<T, MIN, MAX> {
        type WithoutGenerics = List<ts_rs::Dummy, MIN, MAX>;
        type OptionInnerType = Self;
        fn name(cfg: &Config) -> String {
            format!("Array<{}>", T::name(cfg))
        }
        fn inline(cfg: &Config) -> String {
            Self::name(cfg)
        }
        fn visit_dependencies(v: &mut impl TypeVisitor)
        where
            Self: 'static,
        {
            T::visit_dependencies(v);
        }
        fn visit_generics(v: &mut impl TypeVisitor)
        where
            Self: 'static,
        {
            T::visit_generics(v);
            v.visit::<T>();
        }
    }
    impl<T: TS> TS for Nullable<T> {
        type WithoutGenerics = Nullable<ts_rs::Dummy>;
        type OptionInnerType = Self;
        fn name(cfg: &Config) -> String {
            format!("{} | null", T::name(cfg))
        }
        fn inline(cfg: &Config) -> String {
            Self::name(cfg)
        }
        fn visit_dependencies(v: &mut impl TypeVisitor)
        where
            Self: 'static,
        {
            T::visit_dependencies(v);
        }
        fn visit_generics(v: &mut impl TypeVisitor)
        where
            Self: 'static,
        {
            T::visit_generics(v);
            v.visit::<T>();
        }
    }
    impl<const MAX: usize> TS for JsonObject<MAX> {
        type WithoutGenerics = Self;
        type OptionInnerType = Self;
        fn name(_: &Config) -> String {
            "Record<string, unknown>".into()
        }
        fn inline(cfg: &Config) -> String {
            Self::name(cfg)
        }
    }
}

/// Exact boolean literal used in success/error response variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiteralBool<const VALUE: bool>;
impl<const VALUE: bool> Serialize for LiteralBool<VALUE> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(VALUE)
    }
}
impl<'de, const VALUE: bool> Deserialize<'de> for LiteralBool<VALUE> {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        if bool::deserialize(de)? == VALUE {
            Ok(Self)
        } else {
            Err(D::Error::custom("invalid boolean literal"))
        }
    }
}
impl<const VALUE: bool> Validate for LiteralBool<VALUE> {
    fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}
#[cfg(feature = "schema-export")]
impl<const VALUE: bool> schemars::JsonSchema for LiteralBool<VALUE> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        format!("Literal_{VALUE}").into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"const":VALUE})
    }
}
#[cfg(feature = "typescript-export")]
impl<const VALUE: bool> ts_rs::TS for LiteralBool<VALUE> {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    fn name(_: &ts_rs::Config) -> String {
        VALUE.to_string()
    }
    fn inline(cfg: &ts_rs::Config) -> String {
        Self::name(cfg)
    }
}
