use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use platform_core::error::ErrorDetail;
use platform_core::{AppError, AppResult, ErrorCode};
use rand_core::{OsRng, RngCore};
use std::fmt::Write as _;

const MAX_IDENTIFIER_BYTES: usize = 512;
const MIN_PASSWORD_BYTES: usize = 8;
const MAX_PASSWORD_BYTES: usize = 1024;

pub fn normalize_identifier(identifier: &str) -> AppResult<String> {
    let trimmed = identifier.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_IDENTIFIER_BYTES {
        return Err(validation_error("identifier", "identifier is invalid"));
    }

    if trimmed.contains('@') {
        Ok(trimmed.to_ascii_lowercase())
    } else {
        Ok(trimmed.to_owned())
    }
}

pub fn validate_password(password: &str) -> AppResult<()> {
    if password.len() < MIN_PASSWORD_BYTES || password.len() > MAX_PASSWORD_BYTES {
        return Err(validation_error("password", "password is invalid"));
    }
    Ok(())
}

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AppError::new(ErrorCode::Internal, "Password hashing failed"))
}

pub fn verify_password(password_hash: &str, password: &str) -> AppResult<bool> {
    let parsed = PasswordHash::new(password_hash).map_err(|source| {
        AppError::new(
            ErrorCode::Internal,
            format!("Stored password hash is invalid: {source}"),
        )
    })?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

pub fn new_session_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);

    let mut token = String::with_capacity("sess_".len() + bytes.len() * 2);
    token.push_str("sess_");
    for byte in bytes {
        let _ = write!(token, "{byte:02x}");
    }
    token
}

fn validation_error(field: &str, reason: &str) -> AppError {
    AppError::validation(
        "Request validation failed",
        vec![ErrorDetail {
            field: Some(field.to_owned()),
            reason: reason.to_owned(),
        }],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_identifiers_are_trimmed_and_lowercased() {
        assert_eq!(
            normalize_identifier("  Ada@Example.COM  ").expect("identifier should normalize"),
            "ada@example.com"
        );
    }

    #[test]
    fn phone_like_identifiers_are_trimmed_without_lowercasing() {
        assert_eq!(
            normalize_identifier("  +8613800000000  ").expect("identifier should normalize"),
            "+8613800000000"
        );
    }

    #[test]
    fn password_hash_verifies_original_password_only() {
        let hash = hash_password("correct horse").expect("password should hash");

        assert!(verify_password(&hash, "correct horse").expect("hash should verify"));
        assert!(!verify_password(&hash, "wrong horse").expect("hash should verify"));
    }

    #[test]
    fn session_tokens_are_random_enough_for_auth() {
        let first = new_session_token();
        let second = new_session_token();

        assert!(first.starts_with("sess_"));
        assert_eq!(first.len(), "sess_".len() + 64);
        assert_ne!(first, second);
    }
}
