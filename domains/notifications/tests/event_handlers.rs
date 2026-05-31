#[tokio::test]
async fn notifications_registers_user_registered_handler() {
    let pool = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
        .expect("lazy pool should build");
    let descriptor = notifications::module::domain(pool);

    assert_eq!(descriptor.event_handlers.len(), 1);
    assert_eq!(
        descriptor.event_handlers[0].event_name(),
        "identity.user_registered.v1"
    );
    assert!(descriptor
        .runtime
        .functions
        .iter()
        .any(|function| function.name == "notifications.send_welcome_email.v1"));
}
