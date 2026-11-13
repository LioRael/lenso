use std::{error::Error, fmt, future::Future, pin::Pin};

use crate::ActionError;

/// Validated resource configuration and acquisition, preserving the underlying library's value.
pub trait Resource: Sync {
    /// Native configuration supplied by the application or its configuration parser.
    type Config: Send;
    /// Native connection or client, available directly to application code.
    type Value: Send;

    /// Stable requirement identifier used by modules.
    fn id(&self) -> &str;

    /// Checks configuration before attempting connection or other external effects.
    ///
    /// # Errors
    /// Returns an application configuration error when the supplied configuration is invalid.
    fn validate(&self, config: &Self::Config) -> Result<(), ActionError>;

    /// Acquires the native value; partial acquisition failures must clean up their own effects.
    fn open(
        &self,
        config: Self::Config,
    ) -> impl Future<Output = Result<Self::Value, ActionError>> + Send;

    /// Performs explicit asynchronous cleanup. The default uses ordinary Rust ownership and Drop.
    fn close(&self, value: Self::Value) -> impl Future<Output = Result<(), ActionError>> + Send {
        async move {
            drop(value);
            Ok(())
        }
    }
}

/// Validates configuration and returns the resource's original native connection or client.
///
/// # Errors
/// Returns configuration or acquisition failures without replacing their error identity.
pub async fn connect<D: Resource>(
    definition: &D,
    config: D::Config,
) -> Result<D::Value, ActionError> {
    definition.validate(&config)?;
    definition.open(config).await
}

/// Keeps acquisition, use and cleanup failures distinguishable, including simultaneous failures.
#[derive(Debug)]
#[non_exhaustive]
pub enum ResourceError {
    /// Configuration or connection failed; no acquired value is passed to cleanup.
    Open(ActionError),
    /// Application use failed and cleanup succeeded.
    Use(ActionError),
    /// Application use succeeded but cleanup failed.
    Close(ActionError),
    /// Both the application operation and resource cleanup failed.
    UseAndClose {
        /// Original application failure.
        operation: ActionError,
        /// Cleanup failure retained alongside the original operation failure.
        cleanup: ActionError,
    },
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(error) | Self::Use(error) | Self::Close(error) => error.fmt(f),
            Self::UseAndClose { .. } => f.write_str("Resource use and cleanup failed"),
        }
    }
}
impl Error for ResourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::Open(error) | Self::Use(error) | Self::Close(error) => error,
            Self::UseAndClose { operation, .. } => operation,
        })
    }
}

/// Acquires a resource, awaits its user and then awaits cleanup on either return path.
/// The callback borrows the native value so the scope retains ownership for cleanup.
///
/// # Errors
/// Preserves connection, use and cleanup failures, including both failures when they coincide.
pub async fn with_resource<D, F, O>(
    definition: &D,
    config: D::Config,
    use_resource: F,
) -> Result<O, ResourceError>
where
    D: Resource,
    O: Send,
    F: for<'a> FnOnce(
            &'a mut D::Value,
        ) -> Pin<Box<dyn Future<Output = Result<O, ActionError>> + Send + 'a>>
        + Send,
{
    let mut value = connect(definition, config)
        .await
        .map_err(ResourceError::Open)?;
    let result = use_resource(&mut value).await;
    let cleanup = definition.close(value).await;
    match (result, cleanup) {
        (Ok(output), Ok(())) => Ok(output),
        (Err(error), Ok(())) => Err(ResourceError::Use(error)),
        (Ok(_), Err(error)) => Err(ResourceError::Close(error)),
        (Err(operation), Err(cleanup)) => Err(ResourceError::UseAndClose { operation, cleanup }),
    }
}
