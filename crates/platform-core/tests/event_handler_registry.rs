use platform_core::{
    AppError, AppResult, ClaimedOutboxEvent, ErrorCode, EventDispatcher, EventHandler,
    EventHandlerRegistry,
};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn registered_handler_runs_for_matching_event() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = EventHandlerRegistry::new();
    registry.register(Arc::new(CountingHandler::new(
        "identity.user_registered.v1",
        calls.clone(),
    )));

    registry
        .dispatch(&claimed_event("identity.user_registered.v1"))
        .await
        .expect("dispatch should succeed");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unknown_event_is_handled_safely() {
    let registry = EventHandlerRegistry::new();

    registry
        .dispatch(&claimed_event("unknown.event.v1"))
        .await
        .expect("unknown event should be a no-op");
}

#[tokio::test]
async fn multiple_handlers_for_same_event_all_run() {
    let first = Arc::new(AtomicUsize::new(0));
    let second = Arc::new(AtomicUsize::new(0));
    let mut registry = EventHandlerRegistry::new();
    registry.register(Arc::new(CountingHandler::new(
        "identity.user_registered.v1",
        first.clone(),
    )));
    registry.register(Arc::new(CountingHandler::new(
        "identity.user_registered.v1",
        second.clone(),
    )));

    registry
        .dispatch(&claimed_event("identity.user_registered.v1"))
        .await
        .expect("dispatch should succeed");

    assert_eq!(first.load(Ordering::SeqCst), 1);
    assert_eq!(second.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn handler_failure_returns_error_for_relay_retry() {
    let mut registry = EventHandlerRegistry::new();
    registry.register(Arc::new(FailingHandler));

    let error = registry
        .dispatch(&claimed_event("identity.user_registered.v1"))
        .await
        .expect_err("handler failure should bubble up");

    assert_eq!(error.code, ErrorCode::ExternalDependency);
    assert!(error.retryable);
}

#[derive(Debug)]
struct CountingHandler {
    event_name: &'static str,
    calls: Arc<AtomicUsize>,
}

impl CountingHandler {
    fn new(event_name: &'static str, calls: Arc<AtomicUsize>) -> Self {
        Self { event_name, calls }
    }
}

#[async_trait::async_trait]
impl EventHandler for CountingHandler {
    fn event_name(&self) -> &'static str {
        self.event_name
    }

    async fn handle(&self, _event: &ClaimedOutboxEvent) -> AppResult<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Debug)]
struct FailingHandler;

#[async_trait::async_trait]
impl EventHandler for FailingHandler {
    fn event_name(&self) -> &'static str {
        "identity.user_registered.v1"
    }

    async fn handle(&self, _event: &ClaimedOutboxEvent) -> AppResult<()> {
        Err(AppError::new(ErrorCode::ExternalDependency, "handler failed").retryable())
    }
}

fn claimed_event(event_name: impl Into<String>) -> ClaimedOutboxEvent {
    ClaimedOutboxEvent {
        id: "outbox_1".to_owned(),
        event_name: event_name.into(),
        event_version: 1,
        source_module: "identity".to_owned(),
        aggregate_type: "user".to_owned(),
        aggregate_id: "usr_1".to_owned(),
        correlation_id: "corr_1".to_owned(),
        causation_id: None,
        occurred_at: "2026-05-31T00:00:00Z"
            .parse()
            .expect("timestamp should parse"),
        payload: json!({ "user_id": "usr_1" }),
        headers: json!({}),
        attempts: 0,
        max_attempts: 3,
    }
}
