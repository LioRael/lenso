use platform_core::{AppError, ErrorCode};

#[test]
fn error_codes_are_machine_readable() {
    let error = AppError::new(ErrorCode::NotFound, "Resource not found");

    assert_eq!(error.code.as_str(), "not_found");
    assert_eq!(error.public_message, "Resource not found");
    assert!(!error.retryable);
    assert_eq!(error.retry_after_ms, None);
    assert_eq!(error.provider_trace_reference, None);
}

#[test]
fn transport_retry_metadata_is_preserved_without_making_the_error_retryable() {
    let error = AppError::new(ErrorCode::ExternalDependency, "Provider failed")
        .with_retry_after_ms(Some(1_500))
        .with_provider_trace_reference(Some("provider-trace-1".to_owned()));

    assert!(!error.retryable);
    assert_eq!(error.retry_after_ms, Some(1_500));
    assert_eq!(
        error.provider_trace_reference.as_deref(),
        Some("provider-trace-1")
    );
}

#[test]
fn provider_metadata_is_bounded_before_it_reaches_runtime_evidence() {
    let error = AppError::new(ErrorCode::ExternalDependency, "Provider failed")
        .with_retry_after_ms(Some(u64::MAX))
        .with_provider_trace_reference(Some("\n".repeat(1_000)));

    assert_eq!(error.retry_after_ms, Some(86_400_000));
    let reference = error.provider_trace_reference.unwrap();
    assert!(reference.len() <= 512);
    assert!(!reference.chars().any(char::is_control));
}
