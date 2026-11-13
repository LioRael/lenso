use crate::store::{Permission, Store, internal};
use lenso_app::{Action, ActionError, ConsoleExtension, Context, Module, Task, TaskContext};
use lenso_sqlite::rusqlite::params;
use serde::Serialize;
use std::time::Duration;

use crate::items::Change;
pub struct Deliver;
impl Task<Store> for Deliver {
    type Input = Change;
    type Output = ();
    fn id(&self) -> &str {
        "notifications.deliver"
    }
    async fn run(
        &self,
        context: &TaskContext<'_, Store>,
        input: Change,
    ) -> Result<(), ActionError> {
        context.resources.0.queue.with_connection(|db| {
            db.execute("INSERT INTO desk_notifications(job_id,team,recipient,item,actor,kind,version) VALUES(?,?,?,?,?,?,?) ON CONFLICT(job_id) DO NOTHING",params![context.job_id,input.team,input.recipient,input.item,input.actor,input.kind.as_str(),input.version]).map_err(internal)?;
            Ok(())
        })
    }
}
pub struct List;
impl Action<Store> for List {
    type Input = ();
    type Output = Vec<Change>;
    fn id(&self) -> &str {
        "notifications.list"
    }
    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Read)
    }
    async fn run(&self, context: &Context<Store>, (): ()) -> Result<Vec<Change>, ActionError> {
        let store = &context.resources;
        let team = store.team(context)?;
        let subject = &lenso_app::require_principal(context.principal.as_ref())?.subject;
        store.0.queue.with_connection(|db| {
            let mut stmt = db.prepare("SELECT team,recipient,item,actor,kind,version FROM desk_notifications WHERE team=? AND recipient=? ORDER BY item,version").map_err(internal)?;
            let rows = stmt.query_map(params![team,subject],|r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,i64>(5)?))).map_err(internal)?;
            rows.map(|row| { let (team,recipient,item,actor,kind,version) = row.map_err(internal)?; Ok(Change {team,recipient,item,actor,kind:serde_json::from_value(serde_json::Value::String(kind)).map_err(internal)?,version}) }).collect()
        })
    }
}
#[derive(Serialize)]
pub struct Drained {
    completed: u32,
}
pub struct Drain;
impl Action<Store> for Drain {
    type Input = ();
    type Output = Drained;
    fn id(&self) -> &str {
        "notifications.drain"
    }
    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        context.resources.access(context, Permission::Admin)
    }
    async fn run(&self, context: &Context<Store>, (): ()) -> Result<Drained, ActionError> {
        let mut completed = 0;
        while completed < 100
            && context
                .resources
                .0
                .queue
                .run_one(&Deliver, &context.resources, Duration::from_secs(30), 1)
                .await?
                .is_some()
        {
            completed += 1;
        }
        Ok(Drained { completed })
    }
}
pub fn install() -> Result<(Module, ConsoleExtension<Store>), ActionError> {
    let mut console = ConsoleExtension::new();
    console
        .page("notifications", "My notifications", List)?
        .register(Drain)?;
    let mut module = console.module("notifications");
    module.resource("sqlite");
    module.task(&Deliver);
    Ok((module, console))
}
