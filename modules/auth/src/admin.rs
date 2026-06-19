use crate::models::{AuthSessionRecord, AuthUser, AuthUserId};
use crate::repositories::AuthUserRepository;
use chrono::Utc;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{AdminActionSource, AdminDataSource, AdminListQuery, AdminPage};
use serde_json::Value;
use std::sync::Arc;

const REVOKE_SESSION_ACTION: &str = "revoke_session";
const DISABLE_USER_ACTION: &str = "disable_user";
const ENABLE_USER_ACTION: &str = "enable_user";

#[derive(Debug)]
pub struct AuthAdminData {
    repository: Arc<dyn AuthUserRepository>,
}

impl AuthAdminData {
    #[must_use]
    pub fn new(repository: Arc<dyn AuthUserRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl AdminDataSource for AuthAdminData {
    async fn list(&self, entity: &str, query: &AdminListQuery) -> AppResult<AdminPage> {
        match entity {
            "users" => {
                let rows = self
                    .repository
                    .list(query.limit.saturating_add(1), query.cursor.as_deref())
                    .await?;
                let has_more = rows.len() as i64 > query.limit.max(0);
                let take = rows.len().min(query.limit.max(0) as usize);
                let page_rows = &rows[..take];
                let next_cursor = if has_more {
                    page_rows.last().map(|user| user.id.0.clone())
                } else {
                    None
                };
                Ok(AdminPage {
                    records: page_rows.iter().map(user_to_value).collect(),
                    next_cursor,
                })
            }
            "sessions" => {
                let rows = self
                    .repository
                    .list_sessions(query.limit.saturating_add(1), query.cursor.as_deref())
                    .await?;
                let has_more = rows.len() as i64 > query.limit.max(0);
                let take = rows.len().min(query.limit.max(0) as usize);
                let page_rows = &rows[..take];
                let next_cursor = if has_more {
                    page_rows.last().map(|session| session.id.clone())
                } else {
                    None
                };
                Ok(AdminPage {
                    records: page_rows.iter().map(session_to_value).collect(),
                    next_cursor,
                })
            }
            other => Err(unknown_entity(other)),
        }
    }

    async fn get(&self, entity: &str, id: &str) -> AppResult<Option<Value>> {
        match entity {
            "users" => Ok(self
                .repository
                .find_by_id(&AuthUserId(id.to_owned()))
                .await?
                .as_ref()
                .map(user_to_value)),
            "sessions" => Ok(self
                .repository
                .find_session_by_id(id)
                .await?
                .as_ref()
                .map(session_to_value)),
            other => Err(unknown_entity(other)),
        }
    }
}

#[async_trait::async_trait]
impl AdminActionSource for AuthAdminData {
    async fn invoke(&self, action: &str, input: Value) -> AppResult<Value> {
        match action {
            REVOKE_SESSION_ACTION => {
                let session_id = input
                    .get("session_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        AppError::new(ErrorCode::Validation, "session_id is required")
                    })?;
                let revoked = self
                    .repository
                    .revoke_session_by_id(session_id, Utc::now())
                    .await?;
                Ok(serde_json::json!({
                    "session_id": session_id,
                    "revoked": revoked,
                }))
            }
            DISABLE_USER_ACTION => {
                let user_id = action_user_id(&input)?;
                let disabled = self
                    .repository
                    .set_user_disabled_at(&user_id, Some(Utc::now()))
                    .await?;
                Ok(serde_json::json!({
                    "disabled": disabled,
                    "user_id": user_id.0,
                }))
            }
            ENABLE_USER_ACTION => {
                let user_id = action_user_id(&input)?;
                let enabled = self.repository.set_user_disabled_at(&user_id, None).await?;
                Ok(serde_json::json!({
                    "enabled": enabled,
                    "user_id": user_id.0,
                }))
            }
            other => Err(unknown_action(other)),
        }
    }
}

fn action_user_id(input: &Value) -> AppResult<AuthUserId> {
    input
        .get("user_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| AuthUserId(value.to_owned()))
        .ok_or_else(|| AppError::new(ErrorCode::Validation, "user_id is required"))
}

fn unknown_entity(entity: &str) -> AppError {
    AppError::new(
        ErrorCode::NotFound,
        format!("unknown admin entity: {entity}"),
    )
}

fn unknown_action(action: &str) -> AppError {
    AppError::new(
        ErrorCode::NotFound,
        format!("unknown admin action: {action}"),
    )
}

fn user_to_value(user: &AuthUser) -> Value {
    serde_json::json!({
        "id": user.id.0,
        "created_at": user.created_at,
        "disabled_at": user.disabled_at,
    })
}

fn session_to_value(session: &AuthSessionRecord) -> Value {
    serde_json::json!({
        "id": session.id,
        "user_id": session.user_id.0,
        "created_at": session.created_at,
        "expires_at": session.expires_at,
        "revoked_at": session.revoked_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn user_to_value_keys_match_schema_fields() {
        let now = Utc::now();
        let value = user_to_value(&AuthUser {
            id: AuthUserId("usr_1".to_owned()),
            created_at: now,
            disabled_at: None,
        });
        let object = value.as_object().expect("object");
        let mut keys = object.keys().collect::<Vec<_>>();
        keys.sort();
        assert_eq!(keys, vec!["created_at", "disabled_at", "id"]);
    }

    #[test]
    fn session_to_value_keys_match_schema_fields() {
        let now = Utc::now();
        let value = session_to_value(&AuthSessionRecord {
            id: "sess_1".to_owned(),
            user_id: AuthUserId("usr_1".to_owned()),
            created_at: now,
            expires_at: now,
            revoked_at: None,
        });
        let object = value.as_object().expect("object");
        let mut keys = object.keys().collect::<Vec<_>>();
        keys.sort();
        assert_eq!(
            keys,
            vec!["created_at", "expires_at", "id", "revoked_at", "user_id"]
        );
    }
}
