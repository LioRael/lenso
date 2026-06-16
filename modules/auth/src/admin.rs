use crate::models::{AuthUser, AuthUserId};
use crate::repositories::AuthUserRepository;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{AdminDataSource, AdminListQuery, AdminPage};
use serde_json::Value;
use std::sync::Arc;

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
            other => Err(unknown_entity(other)),
        }
    }
}

fn unknown_entity(entity: &str) -> AppError {
    AppError::new(
        ErrorCode::NotFound,
        format!("unknown admin entity: {entity}"),
    )
}

fn user_to_value(user: &AuthUser) -> Value {
    serde_json::json!({
        "id": user.id.0,
        "created_at": user.created_at,
        "disabled_at": user.disabled_at,
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
}
