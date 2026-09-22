//! Source-first Rust authoring support for portable Lenso Capabilities.
//!
//! `#[capability]` turns one typed role trait into an in-memory snapshot. A
//! package build writes that snapshot deliberately during regeneration and
//! otherwise checks the committed Descriptor and Schemas byte-for-byte. Other
//! language bindings continue to generate only from those locked artifacts.
//! An async method returning `Result<Response, DomainError>` declares a request
//! Operation; one returning [`Stream<Message, DomainError>`] declares a stream
//! Operation. Authors do not repeat the interaction kind as a string.

use std::marker::PhantomData;

pub use lenso_contract_authoring_macros::{DomainError, capability};
pub use schemars::JsonSchema;
use schemars::{JsonSchema as JsonSchemaTrait, generate::SchemaSettings};
use serde_json::{Value, json};

/// Product-neutral invocation context marker used in authored Capability traits.
#[derive(Clone, Copy, Debug)]
pub struct Ctx<'a> {
    marker: PhantomData<&'a ()>,
}

/// Authoring marker for one bounded, cancellable Stream Operation.
///
/// The generated runtime projection owns the actual stream session and its
/// Runtime Failure channel. This type records only the portable message and
/// Domain Error contract.
#[derive(Clone, Copy, Debug)]
pub struct Stream<Message, DomainError> {
    marker: PhantomData<(Message, DomainError)>,
}

/// One complete Capability snapshot derived from its owning Rust source.
#[derive(Clone, Debug, PartialEq)]
pub struct CapabilitySnapshot {
    pub capability_id: String,
    pub version: String,
    pub portable: bool,
    pub cross_lane_transfer: bool,
    pub operations: Vec<OperationSnapshot>,
}

/// One request Operation and its derived portable value Schemas.
#[derive(Clone, Debug, PartialEq)]
pub struct OperationSnapshot {
    pub name: String,
    pub interaction: String,
    pub request_schema: Value,
    pub response_schema: Value,
    pub domain_error_schema: Value,
}

/// Generates one closed, inline JSON Schema 2020-12 document from a Rust type.
pub fn schema_for<T: JsonSchemaTrait>() -> Value {
    let settings = SchemaSettings::draft2020_12();
    let schema = settings.into_generator().into_root_schema_for::<T>();
    let mut value = serde_json::to_value(schema).expect("a schemars Schema must serialize");
    normalize_schema(&mut value);
    value
}

/// Supplies a portable Domain Error Schema for one authored enum.
pub trait DomainErrorSchema {
    fn domain_error_schema() -> Value;
}

#[doc(hidden)]
pub fn unit_error_schema(code: &str) -> Value {
    json!({ "const": code })
}

#[doc(hidden)]
pub fn structured_error_schema<T: JsonSchemaTrait>(code: &str) -> Value {
    let mut payload = schema_for::<T>();
    if let Some(payload) = payload.as_object_mut() {
        payload.remove("$schema");
    }
    json!({
        "type": "object",
        "required": ["code", "payload"],
        "properties": {
            "code": { "const": code },
            "payload": payload
        },
        "additionalProperties": false
    })
}

#[doc(hidden)]
pub fn domain_error_union(variants: Vec<Value>) -> Value {
    let mut schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema"
    });
    schema["oneOf"] = Value::Array(variants);
    schema
}

fn normalize_schema(value: &mut Value) {
    match value {
        Value::Object(object) => {
            if matches!(
                object.get("type").and_then(Value::as_str),
                Some("integer" | "number")
            ) {
                // Rust implementation-width hints such as `int64` and
                // `double` are not portable contract formats. Wide portable
                // integers are authored explicitly as formatted strings.
                object.remove("format");
            }
            if object.get("type").and_then(Value::as_str) == Some("object")
                && !object.contains_key("properties")
            {
                object.insert(
                    "properties".to_owned(),
                    Value::Object(serde_json::Map::new()),
                );
            }
            for value in object.values_mut() {
                normalize_schema(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                normalize_schema(value);
            }
        }
        _ => {}
    }
}

/// Implementation details referenced by generated macro expansions.
#[doc(hidden)]
pub mod __private {
    pub use serde_json;
}
