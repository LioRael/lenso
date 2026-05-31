use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Debug, Display};
use thiserror::Error;

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
            details: Vec::new(),
            source: None,
        }
    }

    pub fn validation(public_message: impl Into<String>, details: Vec<ErrorDetail>) -> Self {
        Self {
            code: ErrorCode::Validation,
            public_message: public_message.into(),
            retryable: false,
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
}
