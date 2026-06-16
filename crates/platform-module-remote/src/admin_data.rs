use crate::config::RemoteModuleConfig;
use crate::config::RemoteModuleTransport;
use crate::protocol::{RemoteGetResponse, RemoteListResponse};
use crate::response::decode_json_response;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{AdminDataSource, AdminListQuery, AdminPage};
use serde_json::Value;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RemoteAdminDataSource {
    client: reqwest::Client,
    config: RemoteModuleConfig,
}

impl RemoteAdminDataSource {
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

    async fn send_json<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> AppResult<Option<T>> {
        let response = request.send().await.map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("remote module request failed: {error}"),
            )
            .retryable()
        })?;

        decode_json_response(response, "admin data", true).await
    }
}

#[async_trait::async_trait]
impl AdminDataSource for RemoteAdminDataSource {
    async fn list(&self, entity: &str, query: &AdminListQuery) -> AppResult<AdminPage> {
        if self.config.transport == RemoteModuleTransport::Grpc {
            return crate::grpc::list_admin_records(&self.config, entity, query)
                .await
                .map(Into::into);
        }

        let mut request = self
            .request(reqwest::Method::GET, &format!("admin/{entity}"))
            .query(&[("limit", query.limit.to_string())]);
        if let Some(cursor) = &query.cursor {
            request = request.query(&[("cursor", cursor)]);
        }
        let response = self
            .send_json::<RemoteListResponse>(request)
            .await?
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, "remote admin entity not found"))?;
        Ok(response.into())
    }

    async fn get(&self, entity: &str, id: &str) -> AppResult<Option<Value>> {
        if self.config.transport == RemoteModuleTransport::Grpc {
            return match crate::grpc::get_admin_record(&self.config, entity, id).await {
                Ok(response) => Ok(response.record),
                Err(error) if error.code == ErrorCode::NotFound => Ok(None),
                Err(error) => Err(error),
            };
        }

        let request = self.request(reqwest::Method::GET, &format!("admin/{entity}/{id}"));
        Ok(self
            .send_json::<RemoteGetResponse>(request)
            .await?
            .and_then(|response| response.record))
    }
}
