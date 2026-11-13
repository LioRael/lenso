use crate::items::Change;
use lenso_app::{
    ActionError, CallRecord, Context, DurableEvent, ErrorCode, Principal, require_principal,
};
use lenso_sqlite::{
    SqliteQueue,
    rusqlite::{Connection, OptionalExtension, params},
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn internal(error: impl std::error::Error + Send + Sync + 'static) -> ActionError {
    ActionError::new(ErrorCode::Internal, "Storage operation failed").with_source(error)
}
pub fn fail(code: ErrorCode, message: &str) -> ActionError {
    ActionError::new(code, message)
}
pub fn now_ms() -> Result<i64, ActionError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(internal)?
            .as_millis(),
    )
    .map_err(internal)
}
// Keep copied-name normalization consistent with the TS application's trim parser.
pub fn trim_text(value: &str) -> &str {
    value.trim_matches(|ch| {
        matches!(ch,
            '\u{0009}'..='\u{000d}' | ' ' | '\u{00a0}' | '\u{1680}' |
            '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' |
            '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    })
}
pub fn valid_id(value: &str) -> Result<(), ActionError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(fail(ErrorCode::InvalidInput, "Invalid identifier"));
    }
    Ok(())
}
pub fn valid_version(value: i64) -> Result<(), ActionError> {
    if !(1..=2_147_483_646).contains(&value) {
        return Err(fail(ErrorCode::InvalidInput, "Invalid version"));
    }
    Ok(())
}
pub fn same_version(actual: i64, expected: i64) -> Result<(), ActionError> {
    if actual != expected {
        return Err(fail(
            ErrorCode::Conflict,
            "Version changed; refresh before retrying",
        ));
    }
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Editor,
    Viewer,
}
impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct Identity {
    pub token: String,
    pub team: String,
    pub subject: String,
    pub role: Role,
}
#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub archived: i64,
    pub version: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Open,
    Doing,
    Done,
}
impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Doing => "doing",
            Self::Done => "done",
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub id: String,
    pub project: String,
    pub title: String,
    pub assignee: String,
    pub status: Status,
    pub version: i64,
}
#[derive(Clone)]
pub struct Store(pub Arc<Resources>);
pub struct Resources {
    pub queue: SqliteQueue,
    pub identities: Vec<Identity>,
    pub changed: DurableEvent<Change>,
}
#[derive(Clone, Copy)]
pub enum Permission {
    Read,
    Write,
    Admin,
}
impl Store {
    pub fn open(
        path: &Path,
        identities: Vec<Identity>,
        changed: DurableEvent<Change>,
    ) -> Result<Self, ActionError> {
        if identities.is_empty() {
            return Err(fail(ErrorCode::InvalidInput, "Identities required"));
        }
        let mut tokens = std::collections::HashSet::new();
        for identity in &identities {
            valid_id(&identity.team)?;
            valid_id(&identity.subject)?;
            if identity.token.chars().count() < 16 || !tokens.insert(&identity.token) {
                return Err(fail(
                    ErrorCode::InvalidInput,
                    "Invalid or duplicate identity token",
                ));
            }
        }
        let db = Connection::open(path).map_err(internal)?;
        db.execute_batch(include_str!("../../../../examples/team-desk/schema.sql"))
            .map_err(internal)?;
        for identity in &identities {
            db.execute(
                "INSERT INTO desk_members(team,subject,role) VALUES(?,?,?) ON CONFLICT DO NOTHING",
                params![identity.team, identity.subject, identity.role.as_str()],
            )
            .map_err(internal)?;
        }
        Ok(Self(Arc::new(Resources {
            queue: SqliteQueue::new(db)?,
            identities,
            changed,
        })))
    }
    pub fn authenticate(&self, token: Option<&str>) -> Option<Principal> {
        self.0
            .identities
            .iter()
            .find(|v| Some(v.token.as_str()) == token)
            .map(|v| Principal {
                subject: v.subject.clone(),
                scopes: vec![format!("team:{}", v.team)],
            })
    }
    pub fn team<'a>(&'a self, context: &Context<Self>) -> Result<&'a str, ActionError> {
        let principal = require_principal(context.principal.as_ref())?;
        self.0
            .identities
            .iter()
            .find(|v| {
                v.subject == principal.subject
                    && principal.scopes.contains(&format!("team:{}", v.team))
            })
            .map(|v| v.team.as_str())
            .ok_or_else(|| fail(ErrorCode::Unauthenticated, "Unknown identity"))
    }
    pub fn access(
        &self,
        context: &Context<Self>,
        permission: Permission,
    ) -> Result<(), ActionError> {
        let team = self.team(context)?;
        let subject = &require_principal(context.principal.as_ref())?.subject;
        let role: String = self.0.queue.with_connection(|db| {
            db.query_row(
                "SELECT role FROM desk_members WHERE team=? AND subject=?",
                params![team, subject],
                |row| row.get(0),
            )
            .map_err(internal)
        })?;
        if matches!(permission, Permission::Write) && role == "viewer"
            || matches!(permission, Permission::Admin) && role != "admin"
        {
            return Err(fail(
                ErrorCode::Forbidden,
                "Role does not allow this operation",
            ));
        }
        Ok(())
    }
    pub fn record(&self, record: CallRecord) -> Result<(), ActionError> {
        self.0.queue.with_connection(|db| {
            db.execute(
                "INSERT INTO desk_calls(call_id,action,subject,outcome) VALUES(?,?,?,?)",
                params![
                    record.call_id,
                    record.action,
                    record.subject,
                    record.outcome.err().map_or("ok", ErrorCode::as_str)
                ],
            )
            .map_err(internal)?;
            Ok(())
        })
    }
}
pub fn project(db: &Connection, team: &str, id: &str) -> Result<Project, ActionError> {
    db.query_row(
        "SELECT id,name,archived,version FROM desk_projects WHERE team=? AND id=?",
        params![team, id],
        |r| {
            Ok(Project {
                id: r.get(0)?,
                name: r.get(1)?,
                archived: r.get(2)?,
                version: r.get(3)?,
            })
        },
    )
    .optional()
    .map_err(internal)?
    .ok_or_else(|| fail(ErrorCode::NotFound, "Project not found"))
}
pub fn item(db: &Connection, team: &str, id: &str) -> Result<Item, ActionError> {
    let value = db
        .query_row(
            "SELECT id,project,title,assignee,status,version FROM desk_items WHERE team=? AND id=?",
            params![team, id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(|| fail(ErrorCode::NotFound, "Item not found"))?;
    Ok(Item {
        id: value.0,
        project: value.1,
        title: value.2,
        assignee: value.3,
        status: serde_json::from_value(serde_json::Value::String(value.4)).map_err(internal)?,
        version: value.5,
    })
}
pub fn active(project: &Project) -> Result<(), ActionError> {
    if project.archived != 0 {
        return Err(fail(ErrorCode::Conflict, "Project is archived"));
    }
    Ok(())
}
pub fn assignable(db: &Connection, team: &str, subject: &str) -> Result<(), ActionError> {
    let role: Option<String> = db
        .query_row(
            "SELECT role FROM desk_members WHERE team=? AND subject=?",
            params![team, subject],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?;
    if role.is_none() || role.as_deref() == Some("viewer") {
        return Err(fail(
            ErrorCode::InvalidInput,
            "Assignee must be a team editor or admin",
        ));
    }
    Ok(())
}
