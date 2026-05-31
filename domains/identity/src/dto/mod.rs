use crate::models::user::User;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub email: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateUserResponse {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = CreateUserResponseEnvelope)]
pub struct CreateUserResponseEnvelope {
    pub data: CreateUserResponse,
}

impl From<User> for CreateUserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id.0,
            email: user.email,
            display_name: user.display_name,
            created_at: user.created_at,
        }
    }
}
