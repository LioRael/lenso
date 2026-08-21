use std::fmt;

use crate::{AuthRequest, AuthenticateRequestCredential};

/// Protocol-neutral evidence selected by an ingress Adapter.
pub struct CredentialEvidence {
    wire: AuthenticateRequestCredential,
}

impl CredentialEvidence {
    /// Creates evidence after an ingress Adapter has extracted one credential.
    pub fn new(scheme: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            wire: AuthenticateRequestCredential {
                scheme: scheme.into(),
                value: value.into(),
            },
        }
    }

    /// Returns the protocol-neutral credential scheme.
    pub fn scheme(&self) -> &str {
        &self.wire.scheme
    }

    /// Returns the credential material for the bound Auth Module.
    pub fn value(&self) -> &str {
        &self.wire.value
    }

    fn into_wire(self) -> AuthenticateRequestCredential {
        self.wire
    }
}

impl Clone for CredentialEvidence {
    fn clone(&self) -> Self {
        Self {
            wire: self.wire.clone(),
        }
    }
}

impl fmt::Debug for CredentialEvidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialEvidence")
            .field("scheme", &self.scheme())
            .field("value", &"<redacted>")
            .finish()
    }
}

/// A configured one-scheme credential selection policy owned by an ingress Adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialSelectionPolicy {
    scheme: String,
}

impl CredentialSelectionPolicy {
    /// Configures the one credential scheme this ingress path may select.
    pub fn for_scheme(scheme: impl Into<String>) -> Self {
        Self {
            scheme: scheme.into(),
        }
    }

    /// Selects zero or exactly one matching credential without protocol knowledge.
    pub fn select(
        &self,
        candidates: impl IntoIterator<Item = CredentialEvidence>,
    ) -> Result<Option<CredentialEvidence>, CredentialSelectionError> {
        let mut selected = None;
        for candidate in candidates {
            if candidate.scheme() != self.scheme {
                continue;
            }
            if selected.is_some() {
                return Err(CredentialSelectionError::MultipleConfiguredCredentials {
                    scheme: self.scheme.clone(),
                });
            }
            selected = Some(candidate);
        }
        Ok(selected)
    }
}

/// Error returned when an ingress path violates its one-credential policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CredentialSelectionError {
    /// More than one credential matched the configured scheme.
    MultipleConfiguredCredentials { scheme: String },
}

/// Builds the generated Auth request while keeping protocol extraction outside Auth.
pub fn authenticate_request(evidence: Option<CredentialEvidence>) -> AuthRequest {
    AuthRequest {
        credential: evidence.map(CredentialEvidence::into_wire),
    }
}
