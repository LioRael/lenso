use serde::{Deserialize, Serialize};

pub const USER_REGISTERED: &str = "identity.user_registered.v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserRegisteredV1 {
    pub user_id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub registered_at: String,
}
