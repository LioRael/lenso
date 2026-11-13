use crate::projects::Key;
use crate::store::{self, Permission, Store, fail, internal, valid_id, valid_version};
use lenso_app::{Action, ActionError, ConsoleExtension, Context, ErrorCode, Module};
use lenso_sqlite::rusqlite::params;
use lenso_sqlite::{EnqueueOptions, SqliteQueue, rusqlite::Connection};
use serde::{Deserialize, Serialize};
use store::{Item, Status};
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Created,
    Assigned,
    Status,
}
impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Assigned => "assigned",
            Self::Status => "status",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub team: String,
    pub recipient: String,
    pub item: String,
    pub actor: String,
    pub kind: Kind,
    pub version: i64,
}
#[derive(Deserialize)]
pub struct CreateInput {
    pub id: String,
    pub project: String,
    pub title: String,
    pub assignee: String,
}
#[derive(Deserialize)]
pub struct AssignInput {
    pub id: String,
    pub version: i64,
    pub assignee: String,
}
#[derive(Deserialize)]
pub struct TransitionInput {
    pub id: String,
    pub version: i64,
    pub status: Status,
}
pub fn changed() -> lenso_app::DurableEvent<Change> {
    lenso_app::DurableEvent::new("items.changed")
}
fn publish(
    db: &Connection,
    context: &Context<Store>,
    team: &str,
    id: &str,
    kind: Kind,
) -> Result<Item, ActionError> {
    let item = store::item(db, team, id)?;
    let actor = lenso_app::require_principal(context.principal.as_ref())?
        .subject
        .clone();
    let change = Change {
        team: team.into(),
        recipient: item.assignee.clone(),
        item: id.into(),
        actor,
        kind,
        version: item.version,
    };
    SqliteQueue::publish_on(
        db,
        &context.resources.0.changed,
        &format!("{team}/{id}/{}", item.version),
        &change,
        &EnqueueOptions::default(),
        store::now_ms()?,
    )?;
    Ok(item)
}
pub struct List;
impl Action<Store> for List {
    type Input = ();
    type Output = Vec<Item>;
    fn id(&self) -> &str {
        "items.list"
    }

    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Read)
    }
    async fn run(&self, context: &Context<Store>, input: ()) -> Result<Vec<Item>, ActionError> {
        let () = input;
        let store = &context.resources;
        let team = store.team(context)?;
        store.0.queue.with_connection(|db| {
            let mut stmt = db
                .prepare("SELECT id FROM desk_items WHERE team=? ORDER BY id")
                .map_err(internal)?;
            let ids = stmt
                .query_map([team], |r| r.get::<_, String>(0))
                .map_err(internal)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(internal)?;
            ids.iter().map(|id| store::item(db, team, id)).collect()
        })
    }
}
pub struct Detail;
impl Action<Store> for Detail {
    type Input = Key;
    type Output = Item;
    fn id(&self) -> &str {
        "items.detail"
    }
    fn validate_input(&self, input: &Key) -> Result<(), ActionError> {
        valid_id(&input.id)?;
        Ok(())
    }
    async fn authorize(&self, context: &Context<Store>, _: &Key) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Read)
    }
    async fn run(&self, context: &Context<Store>, input: Key) -> Result<Item, ActionError> {
        let store = &context.resources;
        let team = store.team(context)?;
        store
            .0
            .queue
            .with_connection(|db| store::item(db, team, &input.id))
    }
}
pub struct Create;
impl Action<Store> for Create {
    type Input = CreateInput;
    type Output = Item;
    fn id(&self) -> &str {
        "items.create"
    }
    fn validate_input(&self, input: &CreateInput) -> Result<(), ActionError> {
        valid_id(&input.id)?;
        valid_id(&input.project)?;
        valid_id(&input.assignee)?;
        if store::trim_text(&input.title).is_empty()
            || store::trim_text(&input.title).encode_utf16().count() > 200
        {
            return Err(fail(ErrorCode::InvalidInput, "Invalid item title"));
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
    async fn run(&self, context: &Context<Store>, input: CreateInput) -> Result<Item, ActionError> {
        let store = &context.resources;
        let team = store.team(context)?;
        store.0.queue.with_connection(|db| {
    let tx = db.transaction().map_err(internal)?;
    store::active(&store::project(&tx,team,&input.project)?)?; store::assignable(&tx,team,&input.assignee)?;
    if tx.execute("INSERT INTO desk_items(team,id,project,title,assignee,status) VALUES(?,?,?,?,?,'open') ON CONFLICT DO NOTHING",params![team,input.id,input.project,store::trim_text(&input.title),input.assignee]).map_err(internal)? == 0 { return Err(fail(ErrorCode::Conflict,"Item already exists")); }
    let result = publish(&tx,context,team,&input.id,Kind::Created)?; tx.commit().map_err(internal)?; Ok(result)
})
    }
}
pub struct Assign;
impl Action<Store> for Assign {
    type Input = AssignInput;
    type Output = Item;
    fn id(&self) -> &str {
        "items.assign"
    }
    fn validate_input(&self, input: &AssignInput) -> Result<(), ActionError> {
        valid_id(&input.id)?;
        valid_id(&input.assignee)?;
        valid_version(input.version)?;
        Ok(())
    }
    async fn authorize(
        &self,
        context: &Context<Store>,
        _: &AssignInput,
    ) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Write)
    }
    async fn run(&self, context: &Context<Store>, input: AssignInput) -> Result<Item, ActionError> {
        let store = &context.resources;
        let team = store.team(context)?;
        store.0.queue.with_connection(|db| {
            let tx = db.transaction().map_err(internal)?;
            let item = store::item(&tx, team, &input.id)?;
            store::same_version(item.version, input.version)?;
            store::active(&store::project(&tx, team, &item.project)?)?;
            store::assignable(&tx, team, &input.assignee)?;
            tx.execute(
                "UPDATE desk_items SET assignee=?,version=version+1 WHERE team=? AND id=?",
                params![input.assignee, team, input.id],
            )
            .map_err(internal)?;
            let result = publish(&tx, context, team, &input.id, Kind::Assigned)?;
            tx.commit().map_err(internal)?;
            Ok(result)
        })
    }
}
pub struct Transition;
impl Action<Store> for Transition {
    type Input = TransitionInput;
    type Output = Item;
    fn id(&self) -> &str {
        "items.transition"
    }
    fn validate_input(&self, input: &TransitionInput) -> Result<(), ActionError> {
        valid_id(&input.id)?;
        valid_version(input.version)?;
        Ok(())
    }
    async fn authorize(
        &self,
        context: &Context<Store>,
        _: &TransitionInput,
    ) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Write)
    }
    async fn run(
        &self,
        context: &Context<Store>,
        input: TransitionInput,
    ) -> Result<Item, ActionError> {
        let store = &context.resources;
        let team = store.team(context)?;
        store.0.queue.with_connection(|db| {
            let tx = db.transaction().map_err(internal)?;
            let item = store::item(&tx, team, &input.id)?;
            store::same_version(item.version, input.version)?;
            store::active(&store::project(&tx, team, &item.project)?)?;
            if !matches!(
                (item.status, input.status),
                (Status::Open, Status::Doing) | (Status::Doing, Status::Done)
            ) {
                return Err(fail(ErrorCode::Conflict, "Invalid status transition"));
            }
            tx.execute(
                "UPDATE desk_items SET status=?,version=version+1 WHERE team=? AND id=?",
                params![input.status.as_str(), team, input.id],
            )
            .map_err(internal)?;
            let result = publish(&tx, context, team, &input.id, Kind::Status)?;
            tx.commit().map_err(internal)?;
            Ok(result)
        })
    }
}
pub fn install(
    changed: &lenso_app::DurableEvent<Change>,
) -> Result<(Module, ConsoleExtension<Store>), ActionError> {
    let mut console = ConsoleExtension::new();
    console
        .page("items", "Work items", List)?
        .detail("items", Detail)?
        .operation("items", Create)?
        .operation("items", Assign)?
        .operation("items", Transition)?;
    let mut module = console.module("items");
    module.resource("sqlite");
    module.durable_event(changed);
    Ok((module, console))
}
