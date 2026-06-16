use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordRegisterRequest {
    pub identifier: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordLoginRequest {
    pub identifier: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PasswordSessionResponse {
    pub user_id: String,
    pub session_id: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = PasswordSessionResponseEnvelope)]
pub struct PasswordSessionResponseEnvelope {
    pub data: PasswordSessionResponse,
}
