use crate::{ApiErrorResponse, HttpRequestContext};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use platform_core::{ActorContext, AppError, ErrorCode};

#[derive(Debug, Clone)]
pub struct OptionalActor(pub ActorContext);

#[derive(Debug, Clone)]
pub struct AuthenticatedActor(pub ActorContext);

#[derive(Debug, Clone)]
pub struct UserActor {
    pub user_id: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceActor {
    pub service_id: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum AdminActor {
    Service {
        service_id: String,
        scopes: Vec<String>,
    },
    System,
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for OptionalActor
where
    S: Send + Sync,
{
    type Rejection = ApiErrorResponse;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let HttpRequestContext(ctx) = HttpRequestContext::from_request_parts(parts, state).await?;
        Ok(Self(ctx.actor))
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for AuthenticatedActor
where
    S: Send + Sync,
{
    type Rejection = ApiErrorResponse;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let HttpRequestContext(ctx) = HttpRequestContext::from_request_parts(parts, state).await?;
        match ctx.actor {
            ActorContext::Anonymous => Err(ApiErrorResponse::with_context(
                AppError::new(ErrorCode::Unauthorized, "Authentication is required"),
                &ctx,
            )),
            actor => Ok(Self(actor)),
        }
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for UserActor
where
    S: Send + Sync,
{
    type Rejection = ApiErrorResponse;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let HttpRequestContext(ctx) = HttpRequestContext::from_request_parts(parts, state).await?;
        match ctx.actor {
            ActorContext::Anonymous => Err(ApiErrorResponse::with_context(
                AppError::new(ErrorCode::Unauthorized, "Authentication is required"),
                &ctx,
            )),
            ActorContext::User { user_id, scopes } => Ok(Self { user_id, scopes }),
            ActorContext::Service { .. } | ActorContext::System => {
                Err(ApiErrorResponse::with_context(
                    AppError::new(ErrorCode::Forbidden, "User authentication is required"),
                    &ctx,
                ))
            }
        }
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for ServiceActor
where
    S: Send + Sync,
{
    type Rejection = ApiErrorResponse;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let HttpRequestContext(ctx) = HttpRequestContext::from_request_parts(parts, state).await?;
        match ctx.actor {
            ActorContext::Anonymous => Err(ApiErrorResponse::with_context(
                AppError::new(ErrorCode::Unauthorized, "Authentication is required"),
                &ctx,
            )),
            ActorContext::Service { service_id, scopes } => Ok(Self { service_id, scopes }),
            ActorContext::User { .. } | ActorContext::System => {
                Err(ApiErrorResponse::with_context(
                    AppError::new(ErrorCode::Forbidden, "Service authentication is required"),
                    &ctx,
                ))
            }
        }
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for AdminActor
where
    S: Send + Sync,
{
    type Rejection = ApiErrorResponse;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let HttpRequestContext(ctx) = HttpRequestContext::from_request_parts(parts, state).await?;
        match ctx.actor {
            ActorContext::Anonymous => Err(ApiErrorResponse::with_context(
                AppError::new(ErrorCode::Unauthorized, "Authentication is required"),
                &ctx,
            )),
            ActorContext::Service { service_id, scopes } => {
                Ok(Self::Service { service_id, scopes })
            }
            ActorContext::System => Ok(Self::System),
            ActorContext::User { .. } => Err(ApiErrorResponse::with_context(
                AppError::new(
                    ErrorCode::Forbidden,
                    "Service or system authentication is required",
                ),
                &ctx,
            )),
        }
    }
}
