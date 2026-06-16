use platform_core::{AppError, ErrorCode};

#[test]
fn error_codes_are_machine_readable() {
    let error = AppError::new(ErrorCode::NotFound, "Resource not found");

    assert_eq!(error.code.as_str(), "not_found");
    assert_eq!(error.public_message, "Resource not found");
    assert!(!error.retryable);
}
