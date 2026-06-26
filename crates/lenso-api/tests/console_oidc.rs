use auth_password::config::AuthPasswordConfig;
use auth_password::repositories::{AuthToken, PasswordAuthRepository};
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderName, Request, StatusCode, header};
use chrono::{Duration, Utc};
use lenso_api::build_router;
use platform_core::config::ConsoleConfig;
use platform_core::{
    AppConfig, AppContext, AuthConfig, DatabaseConfig, HttpConfig, LoggingEventPublisher,
    ModuleConfig, ModuleSourcesConfig, RedisConfig, RuntimeConfigProvider, RuntimeConfigRegistry,
    RuntimeConfigSnapshot, ServiceConfig, TelemetryConfig, apply_migrations,
};
use platform_testing::TestDatabase;
use rsa::RsaPrivateKey;
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::rand_core::OsRng;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use tower::ServiceExt;

const CODE_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CODE_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const REDIRECT_URI: &str = "https://console.example.com/console/oidc/callback";

#[tokio::test]
async fn console_oidc_token_carries_password_user_scopes_into_runtime_rbac() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    let config = test_config(&db.url);
    let migrations = lenso_bootstrap::migrations_for_config(&config).expect("migrations compose");
    apply_migrations(&db.pool, &migrations)
        .await
        .expect("migrations apply");

    let admin_password_session = create_password_session(
        &db.pool,
        "usr_console_admin",
        "auth_identity_console_admin",
        "sess_console_admin",
        "console-admin@example.com",
    )
    .await;
    let viewer_password_session = create_password_session(
        &db.pool,
        "usr_console_viewer",
        "auth_identity_console_viewer",
        "sess_console_viewer",
        "console-viewer@example.com",
    )
    .await;
    let ctx = AppContext::new(
        config.clone(),
        db.pool.clone(),
        Arc::new(LoggingEventPublisher),
    );
    let registry = RuntimeConfigRegistry::try_new(
        lenso_bootstrap::runtime_config_descriptors(&ctx).expect("runtime config descriptors"),
    )
    .expect("runtime config registry");
    let mut stored = BTreeMap::new();
    stored.insert(
        ("*".to_owned(), "auth.console_admin_user_scopes".to_owned()),
        json!({
            "usr_console_admin": ["console.admin", "runtime.stories.read"],
            "usr_console_viewer": ["console.admin"]
        }),
    );
    let snapshot = RuntimeConfigSnapshot::resolve(&registry, "api", &stored);
    let ctx = ctx.with_runtime_config_provider(Arc::new(TestRuntimeConfigProvider {
        snapshot: Arc::new(snapshot),
    }));
    let app = build_router(ctx);

    let access_token = oidc_access_token(&app, &admin_password_session).await;

    let context_response = app
        .clone()
        .oneshot(
            get("/admin/context")
                .with_header(header::AUTHORIZATION, &format!("Bearer {access_token}")),
        )
        .await
        .expect("admin context request should complete");
    assert_eq!(context_response.status(), StatusCode::OK);
    let context = json_body(context_response).await;
    assert_eq!(
        context["actor"],
        json!({"kind": "user", "user_id": "usr_console_admin"})
    );
    assert_eq!(
        context["capabilities"],
        json!(["console.admin", "runtime.stories.read"])
    );

    let runtime_response = app
        .clone()
        .oneshot(
            get("/admin/runtime/summary")
                .with_header(header::AUTHORIZATION, &format!("Bearer {access_token}")),
        )
        .await
        .expect("runtime summary request should complete");
    assert_eq!(runtime_response.status(), StatusCode::OK);

    let retry_response = app
        .clone()
        .oneshot(
            post("/admin/runtime/outbox/evt_1/retry")
                .with_header(header::AUTHORIZATION, &format!("Bearer {access_token}")),
        )
        .await
        .expect("runtime retry request should complete");
    assert_eq!(retry_response.status(), StatusCode::FORBIDDEN);

    let viewer_access_token = oidc_access_token(&app, &viewer_password_session).await;
    let viewer_context_response = app
        .clone()
        .oneshot(get("/admin/context").with_header(
            header::AUTHORIZATION,
            &format!("Bearer {viewer_access_token}"),
        ))
        .await
        .expect("viewer admin context request should complete");
    assert_eq!(viewer_context_response.status(), StatusCode::OK);
    let viewer_context = json_body(viewer_context_response).await;
    assert_eq!(
        viewer_context["actor"],
        json!({"kind": "user", "user_id": "usr_console_viewer"})
    );
    assert_eq!(viewer_context["capabilities"], json!(["console.admin"]));

    let viewer_runtime_response = app
        .oneshot(get("/admin/runtime/summary").with_header(
            header::AUTHORIZATION,
            &format!("Bearer {viewer_access_token}"),
        ))
        .await
        .expect("viewer runtime summary request should complete");
    assert_eq!(viewer_runtime_response.status(), StatusCode::FORBIDDEN);

    db.cleanup().await;
}

async fn create_password_session(
    pool: &platform_core::DbPool,
    user_id: &str,
    identity_id: &str,
    session_id: &str,
    identifier: &str,
) -> String {
    let now = Utc::now();
    let token = PasswordAuthRepository::new(pool.clone())
        .register(
            identifier,
            "correct-password",
            user_id.to_owned(),
            identity_id.to_owned(),
            session_id.to_owned(),
            now,
            now + Duration::hours(1),
            &AuthPasswordConfig::default(),
        )
        .await
        .expect("password user should register");
    let AuthToken::Session(session) = token else {
        panic!("default password config should issue a session token");
    };
    session.token
}

async fn oidc_access_token(app: &Router, password_session: &str) -> String {
    let authorize_response = app
        .clone()
        .oneshot(
            get(&format!(
                "/oauth/authorize?response_type=code&client_id=lenso-console&redirect_uri=https%3A%2F%2Fconsole.example.com%2Fconsole%2Foidc%2Fcallback&scope=openid&state=console_state&code_challenge={CODE_CHALLENGE}&code_challenge_method=S256"
            ))
            .with_header(header::COOKIE, &format!("lenso_session={password_session}")),
        )
        .await
        .expect("authorize request should complete");
    assert_eq!(authorize_response.status(), StatusCode::SEE_OTHER);
    let redirect = authorize_response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("authorize response should redirect");
    let code = query_param(redirect, "code");

    let token_response = app
        .clone()
        .oneshot(form_post(
            "/oauth/token",
            &format!(
                "grant_type=authorization_code&code={code}&redirect_uri=https%3A%2F%2Fconsole.example.com%2Fconsole%2Foidc%2Fcallback&client_id=lenso-console&code_verifier={CODE_VERIFIER}"
            ),
        ))
        .await
        .expect("token request should complete");
    assert_eq!(token_response.status(), StatusCode::OK);
    let token = json_body(token_response).await;
    let access_token = token["access_token"]
        .as_str()
        .expect("OIDC token response should include access token");
    assert!(access_token.starts_with("oidc_access_"));
    access_token.to_owned()
}

fn test_config(database_url: &str) -> AppConfig {
    let mut http = HttpConfig::default();
    http.host = "127.0.0.1".to_owned();
    let private_key = test_private_key_pem();
    let mut modules = BTreeMap::new();
    modules.insert(
        "auth-oidc".to_owned(),
        ModuleConfig {
            enabled: None,
            values: BTreeMap::from([
                ("enabled".to_owned(), json!(true)),
                ("issuer".to_owned(), json!("https://api.example.com/")),
                ("console_redirect_uris".to_owned(), json!([REDIRECT_URI])),
                (
                    "jwks".to_owned(),
                    json!({
                        "keys": [{
                            "alg": "RS256",
                            "e": "AQAB",
                            "kid": "test-key",
                            "kty": "RSA",
                            "n": "test-modulus",
                            "use": "sig"
                        }]
                    }),
                ),
                ("id_token_private_key_pem".to_owned(), json!(private_key)),
                ("id_token_key_id".to_owned(), json!("test-key")),
            ]),
        },
    );

    AppConfig {
        auth: AuthConfig::default(),
        console: ConsoleConfig::default(),
        database: DatabaseConfig {
            max_connections: 5,
            url: database_url.to_owned(),
        },
        http,
        module_sources: ModuleSourcesConfig::default(),
        modules,
        redis: RedisConfig::default(),
        service: ServiceConfig {
            environment: "local".to_owned(),
            name: "lenso".to_owned(),
        },
        telemetry: TelemetryConfig::default(),
    }
}

fn test_private_key_pem() -> String {
    RsaPrivateKey::new(&mut OsRng, 2048)
        .expect("test RSA key should generate")
        .to_pkcs8_pem(LineEnding::LF)
        .expect("test RSA key should encode")
        .to_string()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("request should build")
}

fn post(uri: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .body(Body::empty())
        .expect("request should build")
}

fn form_post(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_owned()))
        .expect("request should build")
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    serde_json::from_slice(&bytes).expect("body should be json")
}

fn query_param(uri: &str, name: &str) -> String {
    uri.split_once('?')
        .and_then(|(_, query)| {
            query.split('&').find_map(|part| {
                let (key, value) = part.split_once('=')?;
                (key == name).then(|| value.to_owned())
            })
        })
        .unwrap_or_else(|| panic!("redirect should include {name}"))
}

trait RequestExt {
    fn with_header(self, name: HeaderName, value: &str) -> Self;
}

impl RequestExt for Request<Body> {
    fn with_header(mut self, name: HeaderName, value: &str) -> Self {
        self.headers_mut()
            .insert(name, value.parse().expect("header should parse"));
        self
    }
}

#[derive(Debug)]
struct TestRuntimeConfigProvider {
    snapshot: Arc<RuntimeConfigSnapshot>,
}

impl RuntimeConfigProvider for TestRuntimeConfigProvider {
    fn snapshot(&self) -> Arc<RuntimeConfigSnapshot> {
        Arc::clone(&self.snapshot)
    }
}
