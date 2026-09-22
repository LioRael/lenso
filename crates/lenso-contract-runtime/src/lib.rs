//! Runtime-neutral wire primitives and serde support for generated Lenso contracts.

use std::collections::BTreeMap;

use ::serde::{Serialize, de::DeserializeOwned};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::value::RawValue;

/// Signed 64-bit integer encoded as a decimal string on the wire.
pub type Int64 = String;
/// Unsigned 64-bit integer encoded as a decimal string on the wire.
pub type Uint64 = String;
/// RFC 3339 timestamp encoded as a string on the wire.
pub type Timestamp = String;
/// ISO 8601 duration encoded as a string on the wire.
pub type Duration = String;
/// Distinguishes a missing field from an explicit `null` value.
pub type OptionalValue<T> = Option<Option<T>>;

/// One validated complete JSON value encoded as a JSON string on the wire.
///
/// The wrapper removes string concatenation from contract code while preserving
/// the existing wire shape used by `*_json` fields.
pub struct RawJson(Box<RawValue>);

impl RawJson {
    /// Validates and wraps one complete portable JSON value.
    pub fn new(value: impl Into<String>) -> Result<Self, serde_json::Error> {
        let value = value.into();
        let parsed: serde_json::Value = serde_json::from_str(&value)?;
        validate_portable_json_value(&parsed).map_err(portable_json_error)?;
        RawValue::from_string(value).map(Self)
    }

    /// Returns the exact validated JSON source.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.get()
    }

    /// Returns the owned validated JSON source.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0.get().to_owned()
    }
}

impl Default for RawJson {
    fn default() -> Self {
        Self(RawValue::from_string("null".to_owned()).expect("null is valid JSON"))
    }
}

impl Clone for RawJson {
    fn clone(&self) -> Self {
        Self::new(self.as_str()).expect("RawJson always contains validated JSON")
    }
}

impl std::fmt::Debug for RawJson {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("RawJson")
            .field(&self.as_str())
            .finish()
    }
}

impl PartialEq for RawJson {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for RawJson {}

impl std::str::FromStr for RawJson {
    type Err = serde_json::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for RawJson {
    type Error = serde_json::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for RawJson {
    type Error = serde_json::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl AsRef<str> for RawJson {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::ops::Deref for RawJson {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl std::fmt::Display for RawJson {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for RawJson {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> ::serde::Deserialize<'de> for RawJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        let value = <String as ::serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(::serde::de::Error::custom)
    }
}

/// A typed value encoded as one validated JSON string on the wire.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Json<T>(T);

impl<T> Json<T> {
    /// Wraps a typed value for JSON-string wire encoding.
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Borrows the typed value.
    pub const fn as_inner(&self) -> &T {
        &self.0
    }

    /// Returns the owned typed value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> From<T> for Json<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T> std::ops::Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.as_inner()
    }
}

impl<T: Serialize> Serialize for Json<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        let encoded = encode_portable_json(&self.0).map_err(::serde::ser::Error::custom)?;
        serializer.serialize_str(&encoded)
    }
}

impl<'de, T: DeserializeOwned> ::serde::Deserialize<'de> for Json<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        let encoded = <String as ::serde::Deserialize>::deserialize(deserializer)?;
        decode_portable_json(&encoded)
            .map(Self)
            .map_err(::serde::de::Error::custom)
    }
}

/// Shared native bytes encoded as canonical padded Base64 on the wire.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Bytes(bytes::Bytes);

impl Bytes {
    /// Wraps an owned or shared byte buffer without copying when possible.
    pub fn new(value: impl Into<bytes::Bytes>) -> Self {
        Self(value.into())
    }

    /// Returns the byte slice.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Returns the shared byte buffer without copying.
    #[must_use]
    pub fn into_shared(self) -> bytes::Bytes {
        self.0
    }

    /// Returns an owned vector containing the bytes.
    #[must_use]
    pub fn into_vec(self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(value: Vec<u8>) -> Self {
        Self(value.into())
    }
}

impl From<&[u8]> for Bytes {
    fn from(value: &[u8]) -> Self {
        Self(bytes::Bytes::copy_from_slice(value))
    }
}

impl From<bytes::Bytes> for Bytes {
    fn from(value: bytes::Bytes) -> Self {
        Self(value)
    }
}

impl From<Bytes> for bytes::Bytes {
    fn from(value: Bytes) -> Self {
        value.0
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(value: Bytes) -> Self {
        value.into_vec()
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::ops::Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl Serialize for Bytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(self.as_slice()))
    }
}

impl<'de> ::serde::Deserialize<'de> for Bytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        let encoded = <String as ::serde::Deserialize>::deserialize(deserializer)?;
        STANDARD
            .decode(encoded)
            .map(Self::from)
            .map_err(|_| ::serde::de::Error::custom("bytes must be canonical padded base64"))
    }
}

/// A forward-compatible Capability-defined error unknown to this binding version.
#[derive(Clone, Debug, PartialEq, Serialize, ::serde::Deserialize)]
pub struct UnknownDomainError {
    /// Stable Domain Error code.
    pub code: String,
    /// Optional opaque error payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    /// Additional fields preserved for forward compatibility.
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Encodes a typed value after enforcing the portable JSON number profile.
pub fn encode_portable_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let value = serde_json::to_value(value)?;
    validate_portable_json_value(&value).map_err(portable_json_error)?;
    serde_json::to_string(&value)
}

/// Decodes a typed value after enforcing the portable JSON number profile.
pub fn decode_portable_json<T: DeserializeOwned>(wire: &str) -> Result<T, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(wire)?;
    validate_portable_json_value(&value).map_err(portable_json_error)?;
    serde_json::from_value(value)
}

/// Validates recursively that ordinary JSON numbers are portable across runtimes.
pub fn validate_portable_json_value(value: &serde_json::Value) -> Result<(), String> {
    match value {
        serde_json::Value::Number(number) => {
            let safe = number.as_i64().is_some_and(|value| {
                (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&value)
            }) || number
                .as_u64()
                .is_some_and(|value| value <= 9_007_199_254_740_991)
                || (number.is_f64()
                    && number.as_f64().is_some_and(|value| {
                        value.is_finite()
                            && (value.abs() <= 9_007_199_254_740_991.0 || value.fract() != 0.0)
                    }));
            if !safe {
                return Err("wire JSON contains an unsafe number".to_owned());
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                validate_portable_json_value(value)?;
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                validate_portable_json_value(value)?;
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::String(_) => {}
    }
    Ok(())
}

fn portable_json_error(detail: String) -> serde_json::Error {
    serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, detail))
}

/// Serde helpers used by generated field attributes.
pub mod serde {
    /// Deserializes a required field while allowing generated structs to reject omission.
    pub fn deserialize_required<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: ::serde::Deserializer<'de>,
        T: ::serde::Deserialize<'de>,
    {
        <T as ::serde::Deserialize>::deserialize(deserializer)
    }

    /// Preserves the distinction between a missing field and an explicit `null`.
    #[allow(clippy::option_option)]
    pub fn deserialize_optional_value<'de, D, T>(
        deserializer: D,
    ) -> Result<Option<Option<T>>, D::Error>
    where
        D: ::serde::Deserializer<'de>,
        T: ::serde::Deserialize<'de>,
    {
        Ok(Some(<Option<T> as ::serde::Deserialize>::deserialize(
            deserializer,
        )?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_preserve_canonical_padded_base64_wire_behavior() {
        for length in 0..=258 {
            let value = Bytes::from(
                (0..length)
                    .map(|index| u8::try_from(index % 256).unwrap())
                    .collect::<Vec<_>>(),
            );
            let wire = serde_json::to_string(&value).unwrap();
            assert_eq!(serde_json::from_str::<Bytes>(&wire).unwrap(), value);
        }
        for wire in [r#""not base64""#, r#""AQI""#, r#""AQJ=""#] {
            assert!(serde_json::from_str::<Bytes>(wire).is_err());
        }
    }

    #[test]
    fn shared_bytes_round_trip_without_copying() {
        let source = bytes::Bytes::from_static(b"shared");
        let contract = Bytes::from(source.clone());
        assert_eq!(contract.as_slice().as_ptr(), source.as_ptr());
        let restored = contract.into_shared();
        assert_eq!(restored.as_ptr(), source.as_ptr());
    }

    #[test]
    fn portable_json_rejects_unsafe_integer_values() {
        let error = decode_portable_json::<serde_json::Value>("9007199254740992").unwrap_err();
        assert!(error.to_string().contains("unsafe number"));
    }

    #[test]
    fn raw_json_validates_content_without_changing_the_string_wire_shape() {
        let raw = RawJson::new(r#"{"ready":true}"#).unwrap();
        assert_eq!(
            serde_json::to_string(&raw).unwrap(),
            r#""{\"ready\":true}""#
        );
        assert_eq!(
            serde_json::from_str::<RawJson>(r#""{\"ready\":true}""#).unwrap(),
            raw
        );
        assert!(RawJson::new("not JSON").is_err());
        assert!(RawJson::new("9007199254740992").is_err());
        assert_eq!(raw.as_ref(), r#"{"ready":true}"#);
        assert_eq!(raw.to_string(), r#"{"ready":true}"#);
    }

    #[test]
    fn typed_json_round_trips_as_one_json_string() {
        let value = Json::new(BTreeMap::from([("answer".to_owned(), 42_i32)]));
        let wire = serde_json::to_string(&value).unwrap();
        assert_eq!(wire, r#""{\"answer\":42}""#);
        assert_eq!(
            serde_json::from_str::<Json<BTreeMap<String, i32>>>(&wire).unwrap(),
            value
        );
    }
}
