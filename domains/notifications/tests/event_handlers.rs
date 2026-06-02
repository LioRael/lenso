#[tokio::test]
async fn notifications_registers_user_registered_handler() {
    let pool = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
        .expect("lazy pool should build");
    let ctx = platform_core::AppContext::new(
        platform_core::AppConfig::from_env(),
        pool,
        std::sync::Arc::new(platform_core::LoggingEventPublisher),
    );
    let descriptor = notifications::module::domain(&ctx);

    assert_eq!(descriptor.event_handlers.len(), 1);
    assert_eq!(
        descriptor.event_handlers[0].event_name(),
        "identity.user_registered.v1"
    );
    assert!(
        descriptor
            .runtime
            .functions
            .iter()
            .any(|function| function.name == "notifications.send_welcome_email.v1")
    );
}
