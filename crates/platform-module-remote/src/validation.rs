use platform_core::{AppError, AppResult, ErrorCode};

pub(crate) fn validate_path_segment(value: &str, message: &'static str) -> AppResult<()> {
    let valid = !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || character == '.'
                || character == '_'
                || character == '-'
        });
    if valid {
        return Ok(());
    }

    Err(AppError::new(ErrorCode::Validation, message))
}
