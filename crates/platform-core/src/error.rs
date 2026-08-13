use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Debug, Display};
use thiserror::Error;

const MAX_RETRY_AFTER_MS: u64 = 24 * 60 * 60 * 1_000;
const MAX_PROVIDER_TRACE_REFERENCE_BYTES: usize = 512;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Validation,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    RateLimited,
    ExternalDependency,
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation_failed",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::RateLimited => "rate_limited",
            Self::ExternalDependency => "external_dependency_failure",
            Self::Internal => "internal_error",
        }
    }
}

#[derive(Error)]
pub struct AppError {
    pub code: ErrorCode,
    pub public_message: String,
    pub retryable: bool,
    /// Provider-supplied retry delay hint preserved for the Host retry rail.
    pub retry_after_ms: Option<u64>,
    /// Opaque Provider trace reference suitable for operational correlation.
    pub provider_trace_reference: Option<String>,
    pub details: Vec<ErrorDetail>,
    #[source]
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl Debug for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppError")
            .field("code", &self.code)
            .field("public_message", &self.public_message)
            .field("retryable", &self.retryable)
            .field("retry_after_ms", &self.retry_after_ms)
            .field("provider_trace_reference", &self.provider_trace_reference)
            .field("details", &self.details)
            .finish_non_exhaustive()
    }
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.public_message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ErrorDetail {
    pub field: Option<String>,
    pub reason: String,
}

impl AppError {
    pub fn new(code: ErrorCode, public_message: impl Into<String>) -> Self {
        Self {
            code,
            public_message: public_message.into(),
            retryable: false,
            retry_after_ms: None,
            provider_trace_reference: None,
            details: Vec::new(),
            source: None,
        }
    }

    pub fn validation(public_message: impl Into<String>, details: Vec<ErrorDetail>) -> Self {
        Self {
            code: ErrorCode::Validation,
            public_message: public_message.into(),
            retryable: false,
            retry_after_ms: None,
            provider_trace_reference: None,
            details,
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }

    #[must_use]
    pub fn with_retry_after_ms(mut self, retry_after_ms: Option<u64>) -> Self {
        self.retry_after_ms = retry_after_ms.map(|value| value.min(MAX_RETRY_AFTER_MS));
        self
    }

    #[must_use]
    pub fn with_provider_trace_reference(
        mut self,
        provider_trace_reference: Option<String>,
    ) -> Self {
        self.provider_trace_reference = provider_trace_reference
            .filter(|value| !value.is_empty())
            .map(|value| sanitize_provider_trace_reference(&value));
        self
    }
}

fn sanitize_provider_trace_reference(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len().min(MAX_PROVIDER_TRACE_REFERENCE_BYTES));
    for character in value.chars() {
        let sanitized_character = if character.is_control() {
            '�'
        } else {
            character
        };
        if sanitized.len() + sanitized_character.len_utf8() > MAX_PROVIDER_TRACE_REFERENCE_BYTES {
            break;
        }
        sanitized.push(sanitized_character);
    }
    sanitized
}
