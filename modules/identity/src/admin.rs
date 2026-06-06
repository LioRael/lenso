//! Schema-admin data source: read access to identity's admin entities.

use crate::models::user::{User, UserId};
use crate::repositories::UserRepository;
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::{AdminDataSource, AdminListQuery, AdminPage};
use serde_json::Value;
use std::sync::Arc;

/// identity's read-only admin data source. Holds a `UserRepository` and exposes
/// the "users" entity. Strong `User` types are converted to `Value` only here,
/// at the seam exit.
#[derive(Debug)]
pub struct IdentityAdminData {
    repository: Arc<dyn UserRepository>,
}

impl IdentityAdminData {
    #[must_use]
    pub fn new(repository: Arc<dyn UserRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl AdminDataSource for IdentityAdminData {
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
                    page_rows.last().map(|u| u.id.0.clone())
                } else {
                    None
                };
                let records = page_rows.iter().map(user_to_value).collect();
                Ok(AdminPage {
                    records,
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
                .find_by_id(&UserId(id.to_owned()))
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

/// Strong type → `Value`, ONLY at the boundary. Keys MUST match `user_schema()`.
fn user_to_value(user: &User) -> Value {
    serde_json::json!({
        "id": user.id.0,
        "email": user.email,
        "display_name": user.display_name,
        "created_at": user.created_at,
        "updated_at": user.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn user_to_value_keys_match_schema_fields() {
        let now = Utc::now();
        let user = User {
            id: UserId("usr_1".to_owned()),
            email: "a@example.com".to_owned(),
            display_name: None,
            created_at: now,
            updated_at: now,
        };
        let value = user_to_value(&user);
        let object = value.as_object().expect("object");
        let mut keys: Vec<&String> = object.keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["created_at", "display_name", "email", "id", "updated_at"]
        );
    }
}
