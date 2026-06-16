use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct AuthUserId(pub String);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthUser {
    pub id: AuthUserId,
    pub created_at: DateTime<Utc>,
    pub disabled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthSession {
    pub id: String,
    pub user_id: AuthUserId,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}
