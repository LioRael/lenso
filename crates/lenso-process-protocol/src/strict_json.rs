use std::collections::HashSet;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::ProtocolError;

/// Maximum nested object/array container count accepted by the strict JSON profile.
///
/// `serde_json`'s 128-level recursion limit includes the scalar leaf, leaving
/// room for at most 127 containers around it.
pub const MAX_STRICT_JSON_NESTING: usize = 127;

/// Decodes strict JSON, rejecting duplicate object keys before typed decoding.
pub fn decode_strict<T: DeserializeOwned>(wire: &[u8]) -> Result<T, ProtocolError> {
    serde_json::from_slice::<StrictValue>(wire)
        .map_err(|_| ProtocolError::new("invalid strict JSON"))?;
    serde_json::from_slice(wire).map_err(|_| ProtocolError::new("invalid protocol document"))
}

/// Encodes an already validated protocol document as compact JSON.
pub fn encode_compact<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    serde_json::to_vec(value)
        .map_err(|error| ProtocolError::new(format!("cannot encode protocol document: {error}")))
}

#[derive(Clone, Debug)]
struct StrictValue;

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor)
    }
}

struct StrictValueVisitor;

impl<'de> serde::de::Visitor<'de> for StrictValueVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("strict JSON without duplicate object keys")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }
    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }
    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }
    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }
    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }
    fn visit_string<E>(self, _value: String) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }
    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }
    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        while sequence.next_element::<StrictValue>()?.is_some() {}
        Ok(StrictValue)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(serde::de::Error::custom("duplicate object key"));
            }
            map.next_value::<StrictValue>()?;
        }
        Ok(StrictValue)
    }
}
