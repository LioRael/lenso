use std::{error::Error, fmt};

/// Stable failure classifications across native calls and external adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// Input failed decoding or domain validation.
    InvalidInput,
    /// No authenticated caller was supplied.
    Unauthenticated,
    /// The caller is not permitted to perform this operation.
    Forbidden,
    /// The requested object does not exist.
    NotFound,
    /// The operation conflicts with current state.
    Conflict,
    /// Execution cooperatively stopped after cancellation.
    Cancelled,
    /// An implementation or dependency failed unexpectedly.
    Internal,
}

impl ErrorCode {
    /// Returns the stable external classification.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::Unauthenticated => "unauthenticated",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Cancelled => "cancelled",
            Self::Internal => "internal",
        }
    }
}

/// An application failure with a public message and optional private cause.
#[derive(Debug)]
pub struct ActionError {
    code: ErrorCode,
    message: String,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl ActionError {
    /// Creates an intentional application failure with a safe public message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    /// Retains a private dependency error without exposing it in the public message.
    pub fn with_source(mut self, source: impl Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Returns the stable failure classification.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns the public message, excluding the private source.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ActionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn Error + 'static))
    }
}
