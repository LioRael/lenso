use identity::commands::create_user::IdentityCommands;
use identity::events::USER_REGISTERED;
use identity::models::user::{User, UserId};
use identity::public::{CreateUserCommand, IdentityService};
use identity::repositories::UserRepository;
use platform_core::{
    AppError, AppResult, CorrelationId, ErrorCode, EventEnvelope, EventPublisher, RequestContext,
    RequestId,
};
use platform_testing::{FixedClock, SequentialIdGenerator};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct InMemoryUserRepository {
    users_by_email: Mutex<BTreeMap<String, User>>,
}

#[async_trait::async_trait]
impl UserRepository for InMemoryUserRepository {
    async fn insert(&self, user: &User) -> AppResult<()> {
        let mut users = self
            .users_by_email
            .lock()
            .expect("repository lock poisoned");
        if users.contains_key(&user.email) {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "A user with this email already exists",
            ));
        }

        users.insert(user.email.clone(), user.clone());
        Ok(())
    }

    async fn insert_with_outbox(&self, user: &User, _event: &EventEnvelope) -> AppResult<()> {
        self.insert(user).await
    }

    async fn find_by_id(&self, user_id: &UserId) -> AppResult<Option<User>> {
        let users = self
            .users_by_email
            .lock()
            .expect("repository lock poisoned");
        Ok(users.values().find(|user| &user.id == user_id).cloned())
    }

    async fn find_by_email(&self, email: &str) -> AppResult<Option<User>> {
        let users = self
            .users_by_email
            .lock()
            .expect("repository lock poisoned");
        Ok(users.get(email).cloned())
    }
}

#[derive(Debug, Default)]
struct RecordingEventPublisher {
    events: Mutex<Vec<EventEnvelope>>,
}

#[async_trait::async_trait]
impl EventPublisher for RecordingEventPublisher {
    async fn publish(&self, event: EventEnvelope) -> AppResult<()> {
        self.events
            .lock()
            .expect("event publisher lock poisoned")
            .push(event);
        Ok(())
    }
}

impl RecordingEventPublisher {
    fn event_names(&self) -> Vec<String> {
        self.events
            .lock()
            .expect("event publisher lock poisoned")
            .iter()
            .map(|event| event.event_name.clone())
            .collect()
    }
}

fn request_context() -> RequestContext {
    RequestContext::new(RequestId::new("req_1"), CorrelationId::new("corr_1"))
}

fn fixed_clock() -> FixedClock {
    FixedClock::new(
        "2026-05-31T00:00:00Z"
            .parse()
            .expect("fixed timestamp should parse"),
    )
}

#[tokio::test]
async fn create_user_returns_public_user() {
    let events = Arc::new(RecordingEventPublisher::default());
    let service = IdentityCommands::new(
        Arc::new(InMemoryUserRepository::default()),
        events.clone(),
        Arc::new(fixed_clock()),
        Arc::new(SequentialIdGenerator::default()),
    );
    let ctx = request_context();

    let user = service
        .create_user(
            &ctx,
            CreateUserCommand {
                email: "a@example.com".to_owned(),
                display_name: Some("Ada".to_owned()),
            },
        )
        .await
        .expect("user should be created");

    assert_eq!(user.id.0, "usr_1");
    assert_eq!(user.email, "a@example.com");
    assert_eq!(events.event_names(), vec![USER_REGISTERED.to_owned()]);
}

#[tokio::test]
async fn duplicate_email_returns_conflict() {
    let service = IdentityCommands::new(
        Arc::new(InMemoryUserRepository::default()),
        Arc::new(RecordingEventPublisher::default()),
        Arc::new(fixed_clock()),
        Arc::new(SequentialIdGenerator::default()),
    );
    let ctx = request_context();

    service
        .create_user(
            &ctx,
            CreateUserCommand {
                email: "a@example.com".to_owned(),
                display_name: None,
            },
        )
        .await
        .expect("first user should be created");

    let error = service
        .create_user(
            &ctx,
            CreateUserCommand {
                email: "a@example.com".to_owned(),
                display_name: None,
            },
        )
        .await
        .expect_err("duplicate email should fail");

    assert_eq!(error.code, ErrorCode::Conflict);
}

#[tokio::test]
async fn invalid_email_returns_validation_error() {
    let service = IdentityCommands::new(
        Arc::new(InMemoryUserRepository::default()),
        Arc::new(RecordingEventPublisher::default()),
        Arc::new(fixed_clock()),
        Arc::new(SequentialIdGenerator::default()),
    );
    let ctx = request_context();

    let error = service
        .create_user(
            &ctx,
            CreateUserCommand {
                email: "not-an-email".to_owned(),
                display_name: Some("Ada".to_owned()),
            },
        )
        .await
        .expect_err("invalid email should fail");

    assert_eq!(error.code, ErrorCode::Validation);
}
