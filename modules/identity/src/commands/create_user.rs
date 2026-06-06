use crate::models::user::{User, UserId};
use crate::public::{CreateUserCommand, IdentityService};
use crate::repositories::UserRepository;
use platform_core::{
    AppError, AppResult, Clock, ErrorCode, EventEnvelope, EventPublisher, IdGenerator,
    RequestContext,
};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct IdentityCommands {
    repository: Arc<dyn UserRepository>,
    events: Arc<dyn EventPublisher>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl IdentityCommands {
    pub fn new(
        repository: Arc<dyn UserRepository>,
        events: Arc<dyn EventPublisher>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            repository,
            events,
            clock,
            ids,
        }
    }
}

#[async_trait::async_trait]
impl IdentityService for IdentityCommands {
    async fn create_user(
        &self,
        ctx: &RequestContext,
        command: CreateUserCommand,
    ) -> AppResult<User> {
        let email = normalize_email(&command.email)?;
        if self.repository.find_by_email(&email).await?.is_some() {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "A user with this email already exists",
            ));
        }

        let now = self.clock.now();
        let user = User {
            id: UserId(self.ids.new_id("usr")),
            email,
            display_name: command.display_name,
            created_at: now,
            updated_at: now,
        };

        let event = user_registered_event(ctx, &user);
        self.repository.insert_with_outbox(&user, &event).await?;

        if let Err(error) = self.events.publish(event).await {
            tracing::warn!(error = ?error, "non-transactional event publisher failed");
        }

        Ok(user)
    }

    async fn get_user(&self, _ctx: &RequestContext, user_id: UserId) -> AppResult<User> {
        self.repository.find_by_id(&user_id).await?.ok_or_else(|| {
            AppError::new(
                ErrorCode::NotFound,
                format!("User {} was not found", user_id.0),
            )
        })
    }
}

fn normalize_email(email: &str) -> AppResult<String> {
    let normalized = email.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || !normalized.contains('@')
        || normalized.starts_with('@')
        || normalized.ends_with('@')
    {
        return Err(AppError::new(
            ErrorCode::Validation,
            "Request validation failed",
        ));
    }

    Ok(normalized)
}

fn user_registered_event(ctx: &RequestContext, user: &User) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("evt_{}", user.id.0),
        event_name: crate::events::USER_REGISTERED.to_owned(),
        event_version: 1,
        source_module: "identity".to_owned(),
        subject: user.id.0.clone(),
        tenant_id: ctx.tenant_id.clone(),
        actor: ctx.actor.clone(),
        occurred_at: user.created_at,
        correlation_id: ctx.correlation_id.clone(),
        causation_id: ctx.causation_id.clone(),
        trace: ctx.trace.clone(),
        payload: json!({
            "user_id": user.id.0,
            "email": user.email,
            "display_name": user.display_name,
            "registered_at": user.created_at,
        }),
        schema_ref: "contracts/events/identity/identity.user_registered.v1.schema.json".to_owned(),
    }
}
