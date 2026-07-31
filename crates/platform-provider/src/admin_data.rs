use crate::ProviderHostEffectCoordinator;
use crate::config::ProviderConfig;
use crate::invocation::{self, InvocationContext};
use crate::protocol::{
    ProviderAdminGetRequest, ProviderAdminListRequest, ProviderAdminQueryRequest,
    ProviderGetResponse, ProviderInvocationMode, ProviderListResponse, ProviderOperationKind,
    ProviderQueryResponse,
};
use platform_core::{ActorContext, AppError, AppResult, ErrorCode, TraceContext};
use platform_module::{AdminDataSource, AdminListQuery, AdminPage, AdminQuerySource};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ProviderAdminDataSource {
    client: reqwest::Client,
    config: ProviderConfig,
    effects: ProviderHostEffectCoordinator,
}

impl ProviderAdminDataSource {
    pub fn new(config: ProviderConfig) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to build Provider Service client: {error}"),
                )
            })?;
        Ok(Self {
            client,
            config,
            effects: ProviderHostEffectCoordinator::rejecting(),
        })
    }

    #[must_use]
    pub fn with_effect_coordinator(mut self, effects: ProviderHostEffectCoordinator) -> Self {
        self.effects = effects;
        self
    }

    async fn invoke<T: serde::de::DeserializeOwned>(
        &self,
        kind: ProviderOperationKind,
        binding: &str,
        operation: &str,
        payload: Value,
    ) -> AppResult<T> {
        let invocation_id = uuid::Uuid::now_v7().to_string();
        let invocation = invocation::build(
            &self.config,
            kind,
            operation,
            "1",
            ProviderInvocationMode::ReadOnly,
            InvocationContext {
                request_id: invocation_id.clone(),
                invocation_id,
                attempt: 1,
                actor: ActorContext::System,
                correlation_id: uuid::Uuid::now_v7().to_string(),
                causation_id: None,
                trace: TraceContext::default(),
            },
            payload,
        )?;
        let outcome = invocation::send(
            &self.client,
            &self.config,
            &self.effects,
            binding,
            &invocation,
        )
        .await?;
        serde_json::from_value(invocation::result(&invocation, outcome)?).map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("Provider admin result violated its contract: {error}"),
            )
        })
    }
}

#[async_trait::async_trait]
impl AdminDataSource for ProviderAdminDataSource {
    async fn list(&self, entity: &str, query: &AdminListQuery) -> AppResult<AdminPage> {
        let response: ProviderListResponse = self
            .invoke(
                ProviderOperationKind::AdminList,
                "admin:list",
                entity,
                serde_json::to_value(ProviderAdminListRequest {
                    entity: entity.to_owned(),
                    limit: query.limit,
                    cursor: query.cursor.clone(),
                })
                .map_err(|error| AppError::new(ErrorCode::Internal, error.to_string()))?,
            )
            .await?;
        Ok(response.into())
    }

    async fn get(&self, entity: &str, id: &str) -> AppResult<Option<Value>> {
        let response: ProviderGetResponse = self
            .invoke(
                ProviderOperationKind::AdminGet,
                "admin:get",
                entity,
                serde_json::to_value(ProviderAdminGetRequest {
                    entity: entity.to_owned(),
                    id: id.to_owned(),
                })
                .map_err(|error| AppError::new(ErrorCode::Internal, error.to_string()))?,
            )
            .await?;
        Ok(response.record)
    }
}

#[async_trait::async_trait]
impl AdminQuerySource for ProviderAdminDataSource {
    async fn query(&self, query: &str) -> AppResult<Value> {
        validate_admin_query_name(query)?;
        let response: ProviderQueryResponse = self
            .invoke(
                ProviderOperationKind::AdminQuery,
                "admin:query",
                query,
                serde_json::to_value(ProviderAdminQueryRequest {
                    query: query.to_owned(),
                })
                .map_err(|error| AppError::new(ErrorCode::Internal, error.to_string()))?,
            )
            .await?;
        Ok(response.data)
    }
}

fn validate_admin_query_name(query: &str) -> AppResult<()> {
    let valid = !query.is_empty()
        && query.chars().all(|character| {
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
        "provider admin query name must be a stable path segment",
    ))
}
