use std::time::Duration;
use std::{collections::BTreeMap, fmt, rc::Rc};

use super::{RequestId, lifecycle::CancellationToken};

/// An opaque extension supplied by a caller Plugin.
#[derive(Clone, Eq, PartialEq)]
pub struct InvocationExtension {
    key: String,
    value: Vec<u8>,
}

impl fmt::Debug for InvocationExtension {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InvocationExtension")
            .field("key", &self.key)
            .field("value", &"<redacted>")
            .finish()
    }
}

impl InvocationExtension {
    /// Creates one ordinary, caller-supplied extension value.
    pub fn new(key: impl Into<String>, value: Vec<u8>) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }

    /// Returns the stable extension key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the opaque extension bytes.
    pub fn value(&self) -> &[u8] {
        &self.value
    }
}

/// An opaque extension whose issuer and audience must survive Adapter hops.
#[derive(Clone, Eq, PartialEq)]
pub struct SealedInvocationExtension {
    key: String,
    issuer: String,
    audience: Vec<String>,
    value: Vec<u8>,
    proof: String,
}

impl SealedInvocationExtension {
    /// Carries one domain-signed extension without granting it validity.
    ///
    /// Domain provider bindings must validate `proof` before projecting the
    /// payload. The Kernel preserves the signed fields and prevents replacement.
    pub fn signed(
        key: impl Into<String>,
        issuer: impl Into<String>,
        audience: impl IntoIterator<Item = impl Into<String>>,
        value: Vec<u8>,
        proof: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            issuer: issuer.into(),
            audience: audience.into_iter().map(Into::into).collect(),
            value,
            proof: proof.into(),
        }
    }

    /// Returns the stable extension key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the issuer provenance without interpreting its domain.
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Returns the intended Capability/Operation audience.
    pub fn audience(&self) -> &[String] {
        &self.audience
    }

    /// Returns the opaque extension bytes.
    pub fn value(&self) -> &[u8] {
        &self.value
    }

    /// Returns the domain proof covering issuer, audience, and payload.
    pub fn proof(&self) -> &str {
        &self.proof
    }

    /// Returns whether the signed audience covers one exact target Operation.
    pub fn covers(&self, capability_id: &str, operation: &str) -> bool {
        let target = format!("{capability_id}:{operation}");
        self.audience.iter().any(|audience| audience == &target)
    }
}

impl fmt::Debug for SealedInvocationExtension {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SealedInvocationExtension")
            .field("key", &self.key)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("value", &"<redacted>")
            .field("proof", &"<redacted>")
            .finish()
    }
}

/// Failure returned when an Invocation Context extension cannot be attached.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvocationContextError {
    /// An extension key cannot be empty.
    EmptyExtensionKey,
    /// An ordinary extension already occupies the requested key.
    ExtensionAlreadySet { key: String },
    /// A sealed extension cannot be replaced by another extension value.
    SealedExtensionAlreadySet { key: String },
    /// Sealed provenance must name an issuer and at least one audience entry.
    InvalidSealedExtension { key: String },
}

impl std::fmt::Display for InvocationContextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyExtensionKey => {
                formatter.write_str("Invocation Context extension key is empty")
            }
            Self::ExtensionAlreadySet { key } => {
                write!(
                    formatter,
                    "Invocation Context extension `{key}` is already set"
                )
            }
            Self::SealedExtensionAlreadySet { key } => {
                write!(
                    formatter,
                    "sealed Invocation Context extension `{key}` is already set"
                )
            }
            Self::InvalidSealedExtension { key } => {
                write!(
                    formatter,
                    "sealed Invocation Context extension `{key}` has invalid provenance"
                )
            }
        }
    }
}

/// Kernel-owned context propagated across one native request invocation.
#[derive(Clone, Debug)]
pub struct InvocationContext {
    pub(crate) execution: Option<super::settlement::ExecutionScope>,
    pub(super) caller_instance: Option<Rc<str>>,
    pub(super) request_id: RequestId,
    pub(super) deadline: Option<Duration>,
    pub(super) cancellation: CancellationToken,
    pub(super) extensions: BTreeMap<String, InvocationExtension>,
    pub(super) sealed_extensions: BTreeMap<String, SealedInvocationExtension>,
}

impl InvocationContext {
    /// Retains execution capacity for Adapter-managed work beyond its reply.
    /// The returned lease must be settled on observed termination, not cancellation acknowledgement.
    pub fn retain_execution(&self) -> Result<super::ExecutionLease, super::RuntimeFailure> {
        self.execution
            .as_ref()
            .ok_or(super::RuntimeFailure::AdmissionClosed)?
            .retain()
    }
    /// Creates an invocation context with an absolute Driver-monotonic deadline.
    pub fn new(
        request_id: RequestId,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            execution: None,
            caller_instance: None,
            request_id,
            deadline,
            cancellation,
            extensions: BTreeMap::new(),
            sealed_extensions: BTreeMap::new(),
        }
    }

    /// Attaches the resolved Caller Plugin Instance to this context.
    #[must_use]
    pub fn with_caller_instance(mut self, caller_instance: impl Into<String>) -> Self {
        self.caller_instance = Some(Rc::from(caller_instance.into()));
        self
    }

    pub(crate) fn with_shared_caller_instance(mut self, caller_instance: Rc<str>) -> Self {
        self.caller_instance = Some(caller_instance);
        self
    }

    pub(super) fn for_caller(mut self, caller_instance: &str) -> Self {
        if self.caller_instance.as_deref() != Some(caller_instance) {
            self.caller_instance = Some(Rc::from(caller_instance));
        }
        self
    }

    /// Returns the Caller Plugin Instance, when the App attached one.
    pub fn caller_instance(&self) -> Option<&str> {
        self.caller_instance.as_deref()
    }

    /// Returns the Kernel Request ID used for correlation and cancellation.
    pub const fn request_id(&self) -> RequestId {
        self.request_id
    }

    /// Returns the absolute Driver-monotonic deadline, when one was supplied.
    pub const fn deadline(&self) -> Option<Duration> {
        self.deadline
    }

    /// Returns the caller-owned cooperative cancellation signal.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Adds one ordinary opaque extension without replacing an existing value.
    pub fn with_extension(
        mut self,
        key: impl Into<String>,
        value: Vec<u8>,
    ) -> Result<Self, InvocationContextError> {
        let extension = InvocationExtension::new(key, value);
        if extension.key().is_empty() {
            return Err(InvocationContextError::EmptyExtensionKey);
        }
        if self.sealed_extensions.contains_key(extension.key()) {
            return Err(InvocationContextError::SealedExtensionAlreadySet {
                key: extension.key().to_owned(),
            });
        }
        if self.extensions.contains_key(extension.key()) {
            return Err(InvocationContextError::ExtensionAlreadySet {
                key: extension.key().to_owned(),
            });
        }
        self.extensions
            .insert(extension.key().to_owned(), extension);
        Ok(self)
    }

    /// Adds one sealed extension while preserving issuer, audience, and key ownership.
    pub fn with_sealed_extension(
        mut self,
        extension: SealedInvocationExtension,
    ) -> Result<Self, InvocationContextError> {
        if extension.key().is_empty() {
            return Err(InvocationContextError::EmptyExtensionKey);
        }
        if extension.issuer().is_empty()
            || extension.audience().is_empty()
            || extension.proof().is_empty()
            || extension.audience().iter().any(String::is_empty)
        {
            return Err(InvocationContextError::InvalidSealedExtension {
                key: extension.key().to_owned(),
            });
        }
        if self.sealed_extensions.contains_key(extension.key())
            || self.extensions.contains_key(extension.key())
        {
            return Err(InvocationContextError::SealedExtensionAlreadySet {
                key: extension.key().to_owned(),
            });
        }
        self.sealed_extensions
            .insert(extension.key().to_owned(), extension);
        Ok(self)
    }

    /// Returns one ordinary extension's opaque bytes.
    pub fn extension(&self, key: &str) -> Option<&[u8]> {
        self.extensions.get(key).map(InvocationExtension::value)
    }

    /// Returns ordinary extensions in deterministic key order.
    pub fn extensions(&self) -> impl Iterator<Item = &InvocationExtension> {
        self.extensions.values()
    }

    /// Returns one sealed extension by key.
    pub fn sealed_extension(&self, key: &str) -> Option<&SealedInvocationExtension> {
        self.sealed_extensions.get(key)
    }

    /// Returns sealed extensions in deterministic key order.
    pub fn sealed_extensions(&self) -> impl Iterator<Item = &SealedInvocationExtension> {
        self.sealed_extensions.values()
    }

    /// Restricts sealed extensions to one exact Capability/Operation target.
    ///
    /// Ordinary baggage is preserved. A sealed extension whose audience does
    /// not cover the target is not disclosed to that provider.
    #[must_use]
    pub fn for_target(mut self, capability_id: &str, operation: &str) -> Self {
        self.sealed_extensions
            .retain(|_, extension| extension.covers(capability_id, operation));
        self
    }

    /// Returns whether the caller has already cancelled this invocation.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Returns whether the context deadline has passed at a Driver instant.
    pub fn is_expired(&self, now: Duration) -> bool {
        self.deadline.is_some_and(|deadline| deadline <= now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_caller_reuses_matching_storage_and_overrides_spoofed_identity() {
        let context = InvocationContext::new(1, None, CancellationToken::new())
            .with_caller_instance("consumer".to_owned());
        let original = context.caller_instance().unwrap().as_ptr();
        let context = context.for_caller("consumer");
        assert_eq!(context.caller_instance().unwrap().as_ptr(), original);

        let context = context.for_caller("resolved-consumer");
        assert_eq!(context.caller_instance(), Some("resolved-consumer"));
    }

    #[test]
    fn cloning_context_reuses_caller_storage() {
        let context = InvocationContext::new(1, None, CancellationToken::new())
            .with_caller_instance("consumer".to_owned());
        let cloned = context.clone();

        assert_eq!(
            context.caller_instance().unwrap().as_ptr(),
            cloned.caller_instance().unwrap().as_ptr()
        );
    }
}
