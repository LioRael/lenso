use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDevSessionRequest {
    pub user_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateDevSessionResponse {
    pub user_id: String,
    pub session_id: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = CreateDevSessionResponseEnvelope)]
pub struct CreateDevSessionResponseEnvelope {
    pub data: CreateDevSessionResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RevokeSessionResponse {
    pub revoked: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = RevokeSessionResponseEnvelope)]
pub struct RevokeSessionResponseEnvelope {
    pub data: RevokeSessionResponse,
}
