use crate::store::{self, Permission, Store, fail, internal, valid_id, valid_version};
use lenso_app::{Action, ActionError, ConsoleExtension, Context, ErrorCode, Module};
use lenso_sqlite::rusqlite::params;
use serde::Deserialize;
use store::Project;
#[derive(Deserialize)]
pub struct Key {
    pub id: String,
}
#[derive(Deserialize)]
pub struct CreateInput {
    pub id: String,
    pub name: String,
}
#[derive(Deserialize)]
pub struct ArchiveInput {
    pub id: String,
    pub version: i64,
}
pub struct List;
impl Action<Store> for List {
    type Input = ();
    type Output = Vec<Project>;
    fn id(&self) -> &str {
        "projects.list"
    }

    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Read)
    }
    async fn run(&self, context: &Context<Store>, input: ()) -> Result<Vec<Project>, ActionError> {
        let () = input;
        let store = &context.resources;
        let team = store.team(context)?;
        store.0.queue.with_connection(|db| {
            let mut stmt = db
                .prepare("SELECT id FROM desk_projects WHERE team=? ORDER BY id")
                .map_err(internal)?;
            let ids = stmt
                .query_map([team], |r| r.get::<_, String>(0))
                .map_err(internal)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(internal)?;
            ids.iter().map(|id| store::project(db, team, id)).collect()
        })
    }
}
pub struct Detail;
impl Action<Store> for Detail {
    type Input = Key;
    type Output = Project;
    fn id(&self) -> &str {
        "projects.detail"
    }
    fn validate_input(&self, input: &Key) -> Result<(), ActionError> {
        valid_id(&input.id)?;
        Ok(())
    }
    async fn authorize(&self, context: &Context<Store>, _: &Key) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Read)
    }
    async fn run(&self, context: &Context<Store>, input: Key) -> Result<Project, ActionError> {
        let store = &context.resources;
        let team = store.team(context)?;
        store
            .0
            .queue
            .with_connection(|db| store::project(db, team, &input.id))
    }
}
pub struct Create;
impl Action<Store> for Create {
    type Input = CreateInput;
    type Output = Project;
    fn id(&self) -> &str {
        "projects.create"
    }
    fn validate_input(&self, input: &CreateInput) -> Result<(), ActionError> {
        valid_id(&input.id)?;
        if store::trim_text(&input.name).is_empty()
            || store::trim_text(&input.name).encode_utf16().count() > 120
        {
            return Err(fail(ErrorCode::InvalidInput, "Invalid project name"));
        }
        Ok(())
    }
    async fn authorize(
        &self,
        context: &Context<Store>,
        _: &CreateInput,
    ) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Write)
    }
    async fn run(
        &self,
        context: &Context<Store>,
        input: CreateInput,
    ) -> Result<Project, ActionError> {
        let store = &context.resources;
        let team = store.team(context)?;
        store.0.queue.with_connection(|db| {
            if db
                .execute(
                    "INSERT INTO desk_projects(team,id,name) VALUES(?,?,?) ON CONFLICT DO NOTHING",
                    params![team, input.id, store::trim_text(&input.name)],
                )
                .map_err(internal)?
                == 0
            {
                return Err(fail(ErrorCode::Conflict, "Project already exists"));
            }
            store::project(db, team, &input.id)
        })
    }
}
pub struct Archive;
impl Action<Store> for Archive {
    type Input = ArchiveInput;
    type Output = Project;
    fn id(&self) -> &str {
        "projects.archive"
    }
    fn validate_input(&self, input: &ArchiveInput) -> Result<(), ActionError> {
        valid_id(&input.id)?;
        valid_version(input.version)?;
        Ok(())
    }
    async fn authorize(
        &self,
        context: &Context<Store>,
        _: &ArchiveInput,
    ) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Admin)
    }
    async fn run(
        &self,
        context: &Context<Store>,
        input: ArchiveInput,
    ) -> Result<Project, ActionError> {
        let store = &context.resources;
        let team = store.team(context)?;
        store.0.queue.with_connection(|db| {
            let tx = db.transaction().map_err(internal)?;
            let project = store::project(&tx, team, &input.id)?;
            store::same_version(project.version, input.version)?;
            store::active(&project)?;
            tx.execute(
                "UPDATE desk_projects SET archived=1,version=version+1 WHERE team=? AND id=?",
                params![team, input.id],
            )
            .map_err(internal)?;
            let result = store::project(&tx, team, &input.id)?;
            tx.commit().map_err(internal)?;
            Ok(result)
        })
    }
}
pub fn install() -> Result<(Module, ConsoleExtension<Store>), ActionError> {
    let mut console = ConsoleExtension::new();
    console
        .page("projects", "Projects", List)?
        .detail("projects", Detail)?
        .operation("projects", Create)?
        .operation("projects", Archive)?;
    let mut module = console.module("projects");
    module.resource("sqlite");

    Ok((module, console))
}
