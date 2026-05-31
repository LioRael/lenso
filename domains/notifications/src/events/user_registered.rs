use platform_core::{AppResult, ClaimedOutboxEvent, EventHandler};

pub const USER_REGISTERED: &str = "identity.user_registered.v1";

#[derive(Debug, Default)]
pub struct WelcomeEmailRequestedHandler;

#[async_trait::async_trait]
impl EventHandler for WelcomeEmailRequestedHandler {
    fn event_name(&self) -> &'static str {
        USER_REGISTERED
    }

    async fn handle(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        tracing::info!(
            user_id = %event.aggregate_id,
            correlation_id = %event.correlation_id,
            "welcome email requested"
        );
        Ok(())
    }
}
