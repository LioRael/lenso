#[test]
fn notifications_registers_user_registered_handler() {
    let descriptor = notifications::module::domain();

    assert_eq!(descriptor.event_handlers.len(), 1);
    assert_eq!(
        descriptor.event_handlers[0].event_name(),
        "identity.user_registered.v1"
    );
}
