use lenso_bun_adapter::BunCapabilityCodec;
use lenso_kernel::RuntimeFailure;
use lenso_runtime_codec::JsonCapabilityCodec;
use serde_json::Value;
use std::{
    any::Any,
    collections::{BTreeSet, HashMap},
    sync::{Mutex, OnceLock},
};

pub(crate) fn validate_typed_codec(
    codec: &dyn JsonCapabilityCodec,
    version: &str,
    requests: &[&str],
    streams: &[&str],
) -> Result<(), &'static str> {
    let digest = codec.descriptor_digest();
    let request_set = codec
        .request_operations()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let stream_set = codec
        .stream_operations()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if codec.descriptor_version() != version
        || request_set != requests.iter().copied().collect::<BTreeSet<_>>()
        || stream_set != streams.iter().copied().collect::<BTreeSet<_>>()
        || request_set.len() != codec.request_operations().len()
        || stream_set.len() != codec.stream_operations().len()
        || !codec.event_operations().is_empty()
        || digest.len() != 71
        || !digest.starts_with("sha256:")
        || !digest[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("typed Capability codec differs from the resolved endpoint");
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub(crate) struct PortableJsonCodec {
    pub(crate) capability_id: &'static str,
    pub(crate) descriptor_version: &'static str,
    pub(crate) operations: &'static [&'static str],
}

impl PortableJsonCodec {
    fn encode(&self, operation: &str, value: &dyn Any) -> Result<Value, RuntimeFailure> {
        self.require_operation(operation)?;
        value
            .downcast_ref::<Value>()
            .cloned()
            .ok_or(RuntimeFailure::ProtocolViolation {
                capability: self.capability_id,
            })
    }

    fn decode(&self, operation: &str, value: Value) -> Result<Box<dyn Any>, RuntimeFailure> {
        self.require_operation(operation)?;
        Ok(Box::new(value))
    }

    fn require_operation(&self, operation: &str) -> Result<(), RuntimeFailure> {
        if self.operations.contains(&operation) {
            Ok(())
        } else {
            Err(RuntimeFailure::UnknownOperation {
                capability: self.capability_id,
                operation: operation.to_owned(),
            })
        }
    }
}

impl BunCapabilityCodec for PortableJsonCodec {
    fn capability_id(&self) -> &'static str {
        self.capability_id
    }

    fn descriptor_version(&self) -> &'static str {
        self.descriptor_version
    }

    fn operations(&self) -> &'static [&'static str] {
        self.operations
    }

    fn encode_request(&self, operation: &str, request: &dyn Any) -> Result<Value, RuntimeFailure> {
        self.encode(operation, request)
    }

    fn decode_response(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        self.decode(operation, value)
    }

    fn decode_domain_error(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        self.decode(operation, value)
    }
}

impl JsonCapabilityCodec for PortableJsonCodec {
    fn capability_id(&self) -> &'static str {
        self.capability_id
    }

    fn descriptor_version(&self) -> &'static str {
        self.descriptor_version
    }

    fn request_operations(&self) -> &'static [&'static str] {
        self.operations
    }

    fn encode_request(&self, operation: &str, request: &dyn Any) -> Result<Value, RuntimeFailure> {
        self.encode(operation, request)
    }

    fn decode_response(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        self.decode(operation, value)
    }

    fn decode_domain_error(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        self.decode(operation, value)
    }
}

pub(crate) fn intern_string(value: &str) -> &'static str {
    static VALUES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let values = VALUES.get_or_init(Mutex::default);
    let mut values = values.lock().expect("runtime string interner lock");
    if let Some(value) = values.get(value) {
        return value;
    }
    let interned = Box::leak(value.to_owned().into_boxed_str());
    values.insert(value.to_owned(), interned);
    interned
}

pub(crate) fn intern_operations(operations: &[String]) -> &'static [&'static str] {
    type OperationSets = HashMap<Vec<String>, &'static [&'static str]>;
    static VALUES: OnceLock<Mutex<OperationSets>> = OnceLock::new();
    let values = VALUES.get_or_init(Mutex::default);
    let mut values = values.lock().expect("runtime operation interner lock");
    if let Some(value) = values.get(operations) {
        return value;
    }
    let interned = Box::leak(
        operations
            .iter()
            .map(|operation| intern_string(operation))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    values.insert(operations.to_vec(), interned);
    interned
}
