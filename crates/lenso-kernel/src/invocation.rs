use std::collections::BTreeMap;
use std::time::Duration;

use super::{RequestId, lifecycle::CancellationToken};

/// An opaque extension supplied by a caller Module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationExtension {
    key: String,
    value: Vec<u8>,
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedInvocationExtension {
    key: String,
    issuer: String,
    audience: Vec<String>,
    value: Vec<u8>,
}

impl SealedInvocationExtension {
    /// Creates one sealed extension. Domain Modules own the value format.
    pub fn new(
        key: impl Into<String>,
        issuer: impl Into<String>,
        audience: impl IntoIterator<Item = impl Into<String>>,
        value: Vec<u8>,
    ) -> Self {
        Self {
            key: key.into(),
            issuer: issuer.into(),
            audience: audience.into_iter().map(Into::into).collect(),
            value,
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
    pub(super) caller_instance: Option<String>,
    pub(super) request_id: RequestId,
    pub(super) deadline: Option<Duration>,
    pub(super) cancellation: CancellationToken,
    pub(super) extensions: BTreeMap<String, InvocationExtension>,
    pub(super) sealed_extensions: BTreeMap<String, SealedInvocationExtension>,
}

impl InvocationContext {
    /// Creates an invocation context with an absolute Driver-monotonic deadline.
    pub fn new(
        request_id: RequestId,
        deadline: Option<Duration>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            caller_instance: None,
            request_id,
            deadline,
            cancellation,
            extensions: BTreeMap::new(),
            sealed_extensions: BTreeMap::new(),
        }
    }

    /// Attaches the resolved Caller Module Instance to this context.
    #[must_use]
    pub fn with_caller_instance(mut self, caller_instance: impl Into<String>) -> Self {
        self.caller_instance = Some(caller_instance.into());
        self
    }

    /// Returns the Caller Module Instance, when the App attached one.
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
            || extension
                .audience()
                .iter()
                .any(|audience| audience.is_empty())
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

    /// Returns whether the caller has already cancelled this invocation.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Returns whether the context deadline has passed at a Driver instant.
    pub fn is_expired(&self, now: Duration) -> bool {
        self.deadline.is_some_and(|deadline| deadline <= now)
    }
}
