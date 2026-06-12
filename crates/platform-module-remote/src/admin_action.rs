use crate::config::RemoteModuleConfig;
use crate::protocol::RemoteActionInvokeResponse;
use crate::response::decode_json_response;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::AdminActionSource;
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RemoteAdminActionSource {
    client: reqwest::Client,
    config: RemoteModuleConfig,
}

impl RemoteAdminActionSource {
    pub fn new(config: RemoteModuleConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build remote module client: {error}"),
                )
            })?;
        Ok(Self { client, config })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.config.base_url, path.trim_start_matches('/'))
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let request = self.client.request(method, self.url(path));
        match &self.config.auth_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }
}

#[async_trait::async_trait]
impl AdminActionSource for RemoteAdminActionSource {
    async fn invoke(&self, action: &str, input: Value) -> AppResult<Value> {
        validate_action_name(action)?;
        let response = self
            .request(reqwest::Method::POST, &format!("admin/actions/{action}"))
            .json(&input)
            .send()
            .await
            .map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("remote module action request failed: {error}"),
                )
                .retryable()
            })?;

        let envelope =
            decode_json_response::<RemoteActionInvokeResponse>(response, "admin action", true)
                .await?
                .ok_or_else(|| {
                    AppError::new(ErrorCode::NotFound, "remote admin action not found")
                })?;
        Ok(envelope.result)
    }
}

fn validate_action_name(action: &str) -> AppResult<()> {
    let valid = !action.is_empty()
        && action.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character == '.'
                || character == '_'
                || character == '-'
        });
    if valid {
        return Ok(());
    }

    Err(AppError::new(
        ErrorCode::Validation,
        "remote admin action name must be a stable path segment",
    ))
}
