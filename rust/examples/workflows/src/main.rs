use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use lenso_app::{
    Action, ActionError, CancellationToken, Catalog, ConsoleManifest, ConsolePage, Context,
    DurableEvent, ErrorCode, Event, Executor, Module, Resource, Task, TaskContext, connect,
    require_scope,
};
use lenso_sqlite::{
    EnqueueOptions, SqliteQueue,
    rusqlite::{Connection, params},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

fn internal(error: impl std::error::Error + Send + Sync + 'static) -> ActionError {
    ActionError::new(ErrorCode::Internal, "Workflow operation failed").with_source(error)
}

struct Database;
impl Resource for Database {
    type Config = PathBuf;
    type Value = Connection;
    fn id(&self) -> &str {
        "sqlite"
    }
    fn validate(&self, path: &PathBuf) -> Result<(), ActionError> {
        if path.as_os_str().is_empty() {
            return Err(ActionError::new(
                ErrorCode::InvalidInput,
                "Database path required",
            ));
        }
        Ok(())
    }
    async fn open(&self, path: PathBuf) -> Result<Connection, ActionError> {
        Connection::open(path).map_err(internal)
    }
}

fn now_ms() -> Result<i64, ActionError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(internal)?
            .as_millis(),
    )
    .map_err(internal)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Mail {
    recipient: String,
    body: String,
}

struct Resources {
    queue: SqliteQueue,
    mail: Mailbox,
}
struct Mailbox {
    db: Mutex<Connection>,
}
impl Mailbox {
    fn deliver(&self, id: &str, input: &Mail) -> Result<(), ActionError> {
        let db = self
            .db
            .lock()
            .map_err(|_| ActionError::new(ErrorCode::Internal, "Mailbox lock poisoned"))?;
        db.execute("INSERT INTO mailbox (job_id, recipient, body) VALUES (?, ?, ?) ON CONFLICT(job_id) DO NOTHING", params![id, input.recipient, input.body]).map_err(internal)?;
        Ok(())
    }
}
struct SendMail;
impl Task<Resources> for SendMail {
    type Input = Mail;
    type Output = String;
    fn id(&self) -> &str {
        "mail.deliver"
    }
    async fn run(
        &self,
        context: &TaskContext<'_, Resources>,
        input: Mail,
    ) -> Result<String, ActionError> {
        context.resources.mail.deliver(context.job_id, &input)?;
        Ok("delivered".into())
    }
}

#[derive(Clone)]
struct User {
    id: String,
    name: String,
    email: String,
}
struct Register<'a> {
    created: &'a DurableEvent<User>,
    observed: &'a Event<User>,
}
impl Action<Resources> for Register<'_> {
    type Input = User;
    type Output = String;
    fn id(&self) -> &str {
        "users.register"
    }
    async fn authorize(&self, context: &Context<Resources>, _: &User) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "users:create")
    }
    async fn run(&self, context: &Context<Resources>, input: User) -> Result<String, ActionError> {
        context.resources.queue.with_connection(|db| {
            let transaction = db.transaction().map_err(internal)?;
            transaction
                .execute(
                    "INSERT INTO users (id, name) VALUES (?, ?) ON CONFLICT(id) DO NOTHING",
                    params![input.id, input.name],
                )
                .map_err(internal)?;
            SqliteQueue::publish_on(
                &transaction,
                self.created,
                &input.id,
                &input,
                &EnqueueOptions::default(),
                now_ms()?,
            )?;
            transaction.commit().map_err(internal)
        })?;
        self.observed.publish(input.clone()).await?;
        Ok(input.id)
    }
}

struct Invitation {
    id: String,
    recipient: String,
}
struct Invite;
impl Action<Resources> for Invite {
    type Input = Invitation;
    type Output = String;
    fn id(&self) -> &str {
        "invitations.create"
    }
    async fn authorize(
        &self,
        context: &Context<Resources>,
        _: &Invitation,
    ) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "invitations:create")
    }
    async fn run(
        &self,
        context: &Context<Resources>,
        input: Invitation,
    ) -> Result<String, ActionError> {
        context.resources.queue.with_connection(|db| {
            let transaction = db.transaction().map_err(internal)?;
            transaction.execute("INSERT INTO invitations (id, recipient) VALUES (?, ?) ON CONFLICT(id) DO NOTHING", params![input.id, input.recipient]).map_err(internal)?;
            let mail = Mail { recipient: input.recipient, body: format!("Invitation {}", input.id) };
            let job = SqliteQueue::enqueue_on(&transaction, &SendMail, &mail, &EnqueueOptions { id: Some(format!("invitation/{}", input.id)), ..EnqueueOptions::default() }, now_ms()?)?;
            transaction.commit().map_err(internal)?;
            Ok(job.id)
        })
    }
}

struct ListMail;
impl Action<Resources> for ListMail {
    type Input = ();
    type Output = Vec<Value>;
    fn id(&self) -> &str {
        "console.mailbox"
    }
    async fn authorize(&self, context: &Context<Resources>, _: &()) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "console:read")
    }
    async fn run(&self, context: &Context<Resources>, _: ()) -> Result<Vec<Value>, ActionError> {
        context.resources.queue.with_connection(|db| {
            db.prepare("SELECT job_id, recipient, body FROM mailbox ORDER BY job_id").map_err(internal)?
                .query_map([], |row| Ok(json!({"job_id": row.get::<_, String>(0)?, "recipient": row.get::<_, String>(1)?, "body": row.get::<_, String>(2)?}))).map_err(internal)?
                .collect::<Result<Vec<_>, _>>().map_err(internal)
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filename = PathBuf::from(
        std::env::var("LENSO_DB").unwrap_or_else(|_| ".local/workflows.sqlite".into()),
    );
    if let Some(parent) = filename
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let db = connect(&Database, filename.clone()).await?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, name TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS invitations (id TEXT PRIMARY KEY, recipient TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS mailbox (job_id TEXT PRIMARY KEY, recipient TEXT NOT NULL, body TEXT NOT NULL);")?;
    let context = Context {
        call_id: "workflow-demo".into(),
        cancellation: CancellationToken::new(),
        resources: Resources {
            queue: SqliteQueue::new(db)?,
            mail: Mailbox {
                db: Mutex::new(connect(&Database, filename).await?),
            },
        },
        principal: Some(lenso_app::Principal {
            subject: "local-operator".into(),
            scopes: vec![
                "users:create".into(),
                "invitations:create".into(),
                "console:read".into(),
            ],
        }),
    };
    let local_notifications = std::sync::Arc::new(AtomicUsize::new(0));
    let observed = local_notifications.clone();
    let mut user_created = DurableEvent::new("user.created");
    user_created.subscribe("welcome", &SendMail, |user: &User| Mail {
        recipient: user.email.clone(),
        body: format!("Welcome, {}", user.name),
    })?;
    let mut user_observed = Event::new("user.created.local");
    user_observed.subscribe(move |_| {
        observed.fetch_add(1, Ordering::SeqCst);
        async { Ok(()) }
    });
    let register = Register {
        created: &user_created,
        observed: &user_observed,
    };
    let mut module = Module::new("notifications");
    module
        .action(&register)
        .action(&Invite)
        .action(&ListMail)
        .task(&SendMail)
        .durable_event(&user_created)
        .event(&user_observed)
        .requires(&Database)
        .resource("mailbox");
    let mut catalog = Catalog::new();
    catalog.register(module)?;
    catalog.require_resources(["sqlite", "mailbox"])?;
    let records = Mutex::new(Vec::new());
    let executor = Executor::new(|record| records.lock().expect("record sink lock").push(record));
    executor
        .call(
            &register,
            &context,
            User {
                id: "alice".into(),
                name: "Alice".into(),
                email: "alice@example.test".into(),
            },
        )
        .await?;
    executor
        .call(
            &Invite,
            &context,
            Invitation {
                id: "invite-1".into(),
                recipient: "invitee@example.test".into(),
            },
        )
        .await?;
    let mut completed = 0;
    while context
        .resources
        .queue
        .run_one(&SendMail, &context.resources, Duration::from_secs(30), 1)
        .await?
        .is_some()
    {
        completed += 1;
    }
    let mailbox = executor.call(&ListMail, &context, ()).await?;
    let manifest = ConsoleManifest::new(
        &catalog,
        vec![ConsolePage {
            id: "mailbox".into(),
            title: "Local mailbox".into(),
            list: ListMail.id().into(),
            detail: None,
            actions: vec![],
        }],
    )?;
    println!(
        "{}",
        json!({"mailbox": mailbox, "manifest": manifest, "localNotifications": local_notifications.load(Ordering::SeqCst), "calls": records.lock().expect("record sink lock").len(), "completed": completed})
    );
    Ok(())
}
