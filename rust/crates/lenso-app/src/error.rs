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

/// Public field or array index; never an input value or an unreviewed dynamic map key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
#[cfg_attr(feature = "json", serde(untagged))]
pub enum InputSegment {
    /// A declaration-owned field name.
    Field(String),
    /// An array position.
    Index(usize),
}

/// Fixed public issue classifications, independent of a parser's private error strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
#[cfg_attr(feature = "json", serde(rename_all = "snake_case"))]
pub enum InputIssueCode {
    /// Decoding or validation rejected a value.
    InvalidValue,
    /// A required value is absent.
    Required,
    /// A domain bound was not satisfied.
    OutOfRange,
    /// A declared format was not satisfied.
    InvalidFormat,
}

/// Deliberately public input detail; no received values, parser messages or private causes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct InputIssue {
    /// Public field path; empty means a root error or a path that could not be safely disclosed.
    pub path: Vec<InputSegment>,
    /// Stable classification.
    pub code: InputIssueCode,
}

/// Shared external projection. The original error and its source remain native.
#[derive(Debug)]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
pub struct PublicError<'a> {
    /// Existing stable external classification.
    pub code: &'static str,
    /// Intentionally public application message.
    pub message: &'a str,
    /// Public input details only; omitted for legacy errors and other classifications.
    #[cfg_attr(feature = "json", serde(skip_serializing_if = "slice_empty"))]
    pub issues: &'a [InputIssue],
}

#[cfg(feature = "json")]
fn slice_empty(value: &&[InputIssue]) -> bool {
    value.is_empty()
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
    details: Option<Box<ErrorDetails>>,
}

#[derive(Debug, Default)]
struct ErrorDetails {
    source: Option<Box<dyn Error + Send + Sync>>,
    issues: Vec<InputIssue>,
}

impl ActionError {
    /// Creates an intentional application failure with a safe public message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Retains a private dependency error without exposing it in the public message.
    pub fn with_source(mut self, source: impl Error + Send + Sync + 'static) -> Self {
        self.details.get_or_insert_with(Default::default).source = Some(Box::new(source));
        self
    }

    /// Attaches deliberately public domain validation issues to an input error only.
    pub fn with_issues(mut self, issues: Vec<InputIssue>) -> Self {
        if self.code == ErrorCode::InvalidInput {
            self.details.get_or_insert_with(Default::default).issues = issues;
        }
        self
    }

    /// Returns public input issues without exposing the underlying cause.
    pub fn issues(&self) -> &[InputIssue] {
        self.details
            .as_ref()
            .map_or(&[], |details| details.issues.as_slice())
    }

    /// Projects the same error for HTTP, Console or Agent output without serializing its source.
    pub fn public(&self) -> PublicError<'_> {
        PublicError {
            code: self.code.as_str(),
            message: &self.message,
            issues: self.issues(),
        }
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
        self.details
            .as_ref()?
            .source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn Error + 'static))
    }
}
