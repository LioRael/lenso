mod actions;
mod console;
mod store;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Extension, Json, Router,
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

use store::{Store, internal, random_hex};

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
        call_id: random_hex(16)?,
        principal: store.authenticate(token)?,
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

async fn register(
    State(store): State<Store>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<Value>, HttpError> {
    execute(store, headers, actions::Register, decode_body(&body)?).await
}
async fn login(
    State(store): State<Store>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<Value>, HttpError> {
    execute(store, headers, actions::Login, decode_body(&body)?).await
}
async fn upload(
    State(store): State<Store>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<Value>, HttpError> {
    execute(store, headers, actions::Upload, decode_body(&body)?).await
}
async fn list(State(store): State<Store>, headers: HeaderMap) -> Result<Json<Value>, HttpError> {
    execute(store, headers, actions::List, Value::Null).await
}
async fn read(
    State(store): State<Store>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, HttpError> {
    execute(store, headers, actions::Read, json!({ "id": id })).await
}
fn decode_body(body: &str) -> Result<Value, ActionError> {
    serde_json::from_str(body).map_err(|error| {
        ActionError::new(ErrorCode::InvalidInput, "Invalid JSON").with_source(error)
    })
}

async fn manifest(
    State(store): State<Store>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    execute(store, headers, console::Manifest, Value::Null).await
}
async fn management(
    State(store): State<Store>,
    Extension(console): Extension<Arc<ConsoleExtension<Store>>>,
    headers: HeaderMap,
    Path(action): Path<String>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, HttpError> {
    let context = context(&store, &headers)?;
    Ok(Json(
        console
            .call(&executor(&store), &context, &action, input)
            .await?,
    ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filename = PathBuf::from(
        std::env::var("LENSO_DB").unwrap_or_else(|_| ".local/private-files.sqlite".into()),
    );
    if let Some(parent) = filename
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let store = Store::open(&filename, std::env::var("LENSO_CONSOLE_SUBJECT").ok())?;
    let console = Arc::new(console::extension()?);
    console::catalog(&console)?;
    let app = Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/files", post(upload).get(list))
        .route("/files/{id}", get(read))
        .route("/console/manifest", get(manifest))
        .route("/console/actions/{action}", post(management))
        .layer(Extension(console))
        .layer(DefaultBodyLimit::max(1_500_000))
        .with_state(store);
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3001".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
    println!("Private files: {}", listener.local_addr()?);
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
