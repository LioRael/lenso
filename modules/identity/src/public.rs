use crate::models::user::{User, UserId};
use platform_core::{AppResult, RequestContext};

#[derive(Debug, Clone)]
pub struct CreateUserCommand {
    pub email: String,
    pub display_name: Option<String>,
}

#[async_trait::async_trait]
pub trait IdentityService: Send + Sync {
    async fn create_user(
        &self,
        ctx: &RequestContext,
        command: CreateUserCommand,
    ) -> AppResult<User>;

    async fn get_user(&self, ctx: &RequestContext, user_id: UserId) -> AppResult<User>;
}
