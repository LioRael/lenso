#[tokio::test]
async fn notifications_registers_user_registered_handler() {
    let pool = platform_core::DbPool::connect_lazy("postgres://localhost/lenso_test")
        .expect("lazy pool should build");
    let ctx = platform_core::AppContext::new(
        platform_core::AppConfig::from_env(),
        pool,
        std::sync::Arc::new(platform_core::LoggingEventPublisher),
    );
    let module = notifications::module::module(&ctx);

    let mut event_registry = platform_core::EventHandlerRegistry::new();
    module.binding.register_event_handlers(&mut event_registry);
    assert_eq!(event_registry.handler_count("identity.user_registered.v1"), 1);

    let mut function_registry = platform_runtime::FunctionRegistry::default();
    module.binding.register_functions(&mut function_registry);
    assert!(
        function_registry
            .get("notifications.send_welcome_email.v1")
            .is_some()
    );
}
