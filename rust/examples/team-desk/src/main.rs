mod items;
mod members;
mod notifications;
mod projects;
mod store;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use lenso_app::{
    Action, ActionError, CancellationToken, ConsoleExtension, Context, ErrorCode, Executor,
    Observer,
};
use serde_json::{Value, json};

use store::{Store, internal};
fn call_id() -> Result<String, ActionError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| internal(std::io::Error::other(error.to_string())))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

struct HttpError(ActionError);

impl From<ActionError> for HttpError {
    fn from(error: ActionError) -> Self {
        Self(error)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let code = self.0.code();
        let status = match code {
            ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
            ErrorCode::Unauthenticated => StatusCode::UNAUTHORIZED,
            ErrorCode::Forbidden => StatusCode::FORBIDDEN,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::Conflict => StatusCode::CONFLICT,
            ErrorCode::Cancelled => StatusCode::from_u16(499).expect("valid status"),
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if code == ErrorCode::Internal {
            eprintln!("{:?}", self.0);
        }
        (
            status,
            Json(json!({"error": {"code": code.as_str(), "message": self.0.message()}})),
        )
            .into_response()
    }
}

async fn execute<A: Action<Store>>(
    store: Store,
    headers: HeaderMap,
    action: A,
    input: Value,
) -> Result<Json<Value>, HttpError>
where
    A::Input: serde::de::DeserializeOwned,
    A::Output: serde::Serialize,
{
    let context = context(&store, &headers)?;
    Ok(Json(
        executor(&store).call_json(&action, &context, input).await?,
    ))
}

fn context(store: &Store, headers: &HeaderMap) -> Result<Context<Store>, ActionError> {
    let token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    Ok(Context {
        call_id: call_id()?,
        principal: store.authenticate(token),
        resources: store.clone(),
        cancellation: CancellationToken::new(),
    })
}

fn executor(store: &Store) -> Executor<impl Observer + '_> {
    Executor::new(|record| {
        if let Err(error) = store.record(record) {
            eprintln!("Could not persist call record: {error:?}");
        }
    })
}

fn decode_body(body: &str) -> Result<Value, ActionError> {
    serde_json::from_str(body).map_err(|error| {
        ActionError::new(ErrorCode::InvalidInput, "Invalid JSON").with_source(error)
    })
}

#[derive(Clone)]
struct Application {
    store: Store,
    console: Arc<ConsoleExtension<Store>>,
    catalog: Arc<lenso_app::Catalog>,
}
async fn manifest(
    State(app): State<Application>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    let context = context(&app.store, &headers)?;
    let action = Manifest {
        catalog: app.catalog.clone(),
        console: app.console.clone(),
    };
    Ok(Json(
        executor(&app.store)
            .call_json(&action, &context, Value::Null)
            .await?,
    ))
}
struct Manifest {
    catalog: Arc<lenso_app::Catalog>,
    console: Arc<ConsoleExtension<Store>>,
}
impl Action<Store> for Manifest {
    type Input = ();
    type Output = lenso_app::ConsoleManifest;
    fn id(&self) -> &str {
        "console.manifest"
    }
    async fn authorize(&self, context: &Context<Store>, _: &()) -> Result<(), ActionError> {
        context.resources.access(context, store::Permission::Read)
    }
    async fn run(&self, _: &Context<Store>, (): ()) -> Result<Self::Output, ActionError> {
        self.console.manifest(&self.catalog)
    }
}
async fn drain(
    State(app): State<Application>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<Value>, HttpError> {
    execute(
        app.store,
        headers,
        notifications::Drain,
        decode_body(&body)?,
    )
    .await
}
async fn management(
    State(app): State<Application>,
    headers: HeaderMap,
    Path(action): Path<String>,
    body: String,
) -> Result<Json<Value>, HttpError> {
    let context = context(&app.store, &headers)?;
    Ok(Json(
        app.console
            .call(
                &executor(&app.store),
                &context,
                &action,
                decode_body(&body)?,
            )
            .await?,
    ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filename = PathBuf::from(
        std::env::var("LENSO_DB").unwrap_or_else(|_| ".local/team-desk.sqlite".into()),
    );
    if let Some(parent) = filename
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let identities = serde_json::from_str(&std::env::var("LENSO_IDENTITIES")?)?;
    let mut changed = items::changed();
    changed.subscribe("inbox", &notifications::Deliver, |change| change.clone())?;
    let store = Store::open(&filename, identities, changed)?;
    let mut console = ConsoleExtension::new();
    let mut catalog = lenso_app::Catalog::new();
    for (module, extension) in [
        members::install()?,
        projects::install()?,
        items::install(&store.0.changed)?,
        notifications::install()?,
    ] {
        catalog.register(module)?;
        console.merge(extension)?;
    }
    catalog.require_resources(["sqlite"])?;
    console.manifest(&catalog)?;
    let app = Router::new()
        .route("/console/manifest", get(manifest))
        .route("/console/actions/{action}", post(management))
        .route("/worker/drain", post(drain))
        .layer(DefaultBodyLimit::max(1_500_000))
        .with_state(Application {
            store,
            console: Arc::new(console),
            catalog: Arc::new(catalog),
        });
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3001".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
    println!("Team Desk: {}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(internal)?;
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut terminate) = signal(SignalKind::terminate()) {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use store::{Identity, Role};

    #[tokio::test]
    async fn native_calls_keep_domain_types_and_projects_install_independently()
    -> Result<(), ActionError> {
        let identities = vec![Identity {
            token: "native-test-secret".into(),
            team: "alpha".into(),
            subject: "alice".into(),
            role: Role::Admin,
        }];
        let store = Store::open(
            std::path::Path::new(":memory:"),
            identities,
            items::changed(),
        )?;
        let mut context = Context {
            call_id: "native".into(),
            principal: store.authenticate(Some("native-test-secret")),
            resources: store.clone(),
            cancellation: CancellationToken::new(),
        };
        context
            .principal
            .as_mut()
            .expect("authenticated fixture")
            .scopes
            .insert(0, "console:read".into());
        let executor = executor(&store);
        let project = executor
            .call(
                &projects::Create,
                &context,
                projects::CreateInput {
                    id: "p1".into(),
                    name: "Native project".into(),
                },
            )
            .await?;
        assert_eq!(project.version, 1);
        let item = executor
            .call(
                &items::Create,
                &context,
                items::CreateInput {
                    id: "i1".into(),
                    project: project.id,
                    title: "Native item".into(),
                    assignee: "alice".into(),
                },
            )
            .await?;
        assert_eq!(item.status, store::Status::Open);
        let (module, console) = projects::install()?;
        let mut catalog = lenso_app::Catalog::new();
        catalog.register(module)?;
        assert_eq!(console.manifest(&catalog)?.modules.len(), 1);
        assert_eq!(
            console
                .call(&executor, &context, "items.list", Value::Null)
                .await
                .expect_err("item module is absent")
                .code(),
            ErrorCode::NotFound
        );
        Ok(())
    }
}
