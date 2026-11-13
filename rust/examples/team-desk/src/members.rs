use crate::store::{self, Permission, Store, fail, internal, valid_id};
use lenso_app::{Action, ActionError, ConsoleExtension, Context, ErrorCode, Module};
use lenso_sqlite::rusqlite::params;
use serde::{Deserialize, Serialize};
#[derive(Serialize)]
pub struct Member {
    subject: String,
    role: store::Role,
}
#[derive(Deserialize)]
pub struct SetRoleInput {
    pub subject: String,
    pub role: store::Role,
}
#[derive(Serialize)]
pub struct SessionInfo {
    subject: String,
    team: String,
    role: store::Role,
}
pub struct Session;
impl Action<Store> for Session {
    type Input = ();
    type Output = SessionInfo;
    fn id(&self) -> &str {
        "members.session"
    }
    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Read)
    }
    async fn run(&self, context: &Context<Store>, (): ()) -> Result<SessionInfo, ActionError> {
        let store = &context.resources;
        let team = store.team(context)?;
        let subject = &lenso_app::require_principal(context.principal.as_ref())?.subject;
        store.0.queue.with_connection(|db| {
            let role: String = db
                .query_row(
                    "SELECT role FROM desk_members WHERE team=? AND subject=?",
                    params![team, subject],
                    |row| row.get(0),
                )
                .map_err(internal)?;
            Ok(SessionInfo {
                subject: subject.clone(),
                team: team.into(),
                role: serde_json::from_value(serde_json::Value::String(role)).map_err(internal)?,
            })
        })
    }
}
pub struct List;
impl Action<Store> for List {
    type Input = ();
    type Output = Vec<Member>;
    fn id(&self) -> &str {
        "members.list"
    }

    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Read)
    }
    async fn run(&self, context: &Context<Store>, input: ()) -> Result<Vec<Member>, ActionError> {
        let () = input;
        let store = &context.resources;
        let team = store.team(context)?;
        store.0.queue.with_connection(|db| {
            let mut stmt = db
                .prepare("SELECT subject,role FROM desk_members WHERE team=? ORDER BY subject")
                .map_err(internal)?;
            let rows = stmt
                .query_map([team], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(internal)?;
            rows.map(|row| {
                let (subject, role) = row.map_err(internal)?;
                Ok(Member {
                    subject,
                    role: serde_json::from_value(serde_json::Value::String(role))
                        .map_err(internal)?,
                })
            })
            .collect()
        })
    }
}
pub struct SetRole;
impl Action<Store> for SetRole {
    type Input = SetRoleInput;
    type Output = Member;
    fn id(&self) -> &str {
        "members.setRole"
    }
    fn validate_input(&self, input: &SetRoleInput) -> Result<(), ActionError> {
        valid_id(&input.subject)?;
        Ok(())
    }
    async fn authorize(
        &self,
        context: &Context<Store>,
        _: &SetRoleInput,
    ) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Admin)
    }
    async fn run(
        &self,
        context: &Context<Store>,
        input: SetRoleInput,
    ) -> Result<Member, ActionError> {
        let store = &context.resources;
        let team = store.team(context)?;
        if context
            .principal
            .as_ref()
            .is_some_and(|p| p.subject == input.subject)
        {
            return Err(fail(ErrorCode::Conflict, "Cannot change your own role"));
        }
        store.0.queue.with_connection(|db| {
            if db
                .execute(
                    "UPDATE desk_members SET role=? WHERE team=? AND subject=?",
                    params![input.role.as_str(), team, input.subject],
                )
                .map_err(internal)?
                == 0
            {
                return Err(fail(ErrorCode::NotFound, "Member not found"));
            }
            Ok(Member {
                subject: input.subject,
                role: input.role,
            })
        })
    }
}
pub fn install() -> Result<(Module, ConsoleExtension<Store>), ActionError> {
    let mut console = ConsoleExtension::new();
    console
        .page("members", "Members", List)?
        .operation("members", SetRole)?
        .register(Session)?;
    let mut module = console.module("members");
    module.resource("sqlite");

    Ok((module, console))
}
