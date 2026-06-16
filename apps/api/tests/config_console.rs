use app_api::build_router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use platform_core::{
    AppConfig, AppContext, DatabaseConfig, LoggingEventPublisher, PLATFORM_MIGRATIONS,
    PostgresRuntimeConfigProvider, RuntimeConfigDescriptor, RuntimeConfigGeneratedValue,
    RuntimeConfigGroupDescriptor, RuntimeConfigRegistry, RuntimeConfigScope, RuntimeConfigType,
    RuntimeConfigVisibilityCondition, apply_migrations,
};
use platform_testing::TestDatabase;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

fn registry() -> RuntimeConfigRegistry {
    RuntimeConfigRegistry::try_new_with_groups(
        vec![
            RuntimeConfigDescriptor {
                key: "demo.flag".to_owned(),
                scope: RuntimeConfigScope::Shared,
                group: Some("demo"),
                section: Some("Basics"),
                order: 10,
                visible_when: None,
                generated: None,
                value_type: RuntimeConfigType::Bool,
                default: json!(false),
                editable: true,
                restart_only: false,
                description: "demo flag",
            },
            RuntimeConfigDescriptor {
                key: "demo.locked".to_owned(),
                scope: RuntimeConfigScope::Shared,
                group: Some("demo"),
                section: Some("Basics"),
                order: 20,
                visible_when: Some(RuntimeConfigVisibilityCondition::Equals {
                    service: "*",
                    key: "demo.flag",
                    value: json!(true),
                }),
                generated: None,
                value_type: RuntimeConfigType::Bool,
                default: json!(true),
                editable: false,
                restart_only: false,
                description: "non-editable demo flag",
            },
            RuntimeConfigDescriptor {
                key: "demo.mode".to_owned(),
                scope: RuntimeConfigScope::Shared,
                group: Some("demo"),
                section: Some("Generated"),
                order: 30,
                visible_when: None,
                generated: None,
                value_type: RuntimeConfigType::Enum(&["basic", "secret"]),
                default: json!("basic"),
                editable: true,
                restart_only: true,
                description: "demo mode",
            },
            RuntimeConfigDescriptor {
                key: "demo.secret".to_owned(),
                scope: RuntimeConfigScope::Shared,
                group: Some("demo"),
                section: Some("Generated"),
                order: 40,
                visible_when: Some(RuntimeConfigVisibilityCondition::Equals {
                    service: "*",
                    key: "demo.mode",
                    value: json!("secret"),
                }),
                generated: Some(RuntimeConfigGeneratedValue::Secret {
                    bytes: 32,
                    when: RuntimeConfigVisibilityCondition::Equals {
                        service: "*",
                        key: "demo.mode",
                        value: json!("secret"),
                    },
                }),
                value_type: RuntimeConfigType::String,
                default: json!(null),
                editable: true,
                restart_only: false,
                description: "generated demo secret",
            },
        ],
        vec![RuntimeConfigGroupDescriptor {
            id: "demo",
            label: "Demo",
            description: "Demo settings.",
            order: 10,
        }],
    )
    .unwrap()
}

fn req(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request builds")
}

fn req_json(method: &str, uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(body).expect("serialize body"),
        ))
        .expect("request builds")
}

trait RequestExt {
    fn with_admin(self) -> Self;
}
impl RequestExt for Request<Body> {
    fn with_admin(mut self) -> Self {
        self.headers_mut()
            .insert("authorization", "Bearer dev-service:admin".parse().unwrap());
        self
    }
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn config_console_round_trip() {
    let Some(db) = TestDatabase::create().await else {
        return;
    };
    apply_migrations(&db.pool, PLATFORM_MIGRATIONS)
        .await
        .expect("migrations apply");

    let reg = registry();
    platform_admin::install_runtime_config_registry(reg.clone());

    let mut config = AppConfig::from_env();
    config.database = DatabaseConfig {
        url: db.url.clone(),
        max_connections: 5,
    };
    let mut ctx = AppContext::new(config, db.pool.clone(), Arc::new(LoggingEventPublisher));
    let settings = PostgresRuntimeConfigProvider::connect(db.pool.clone(), Arc::new(reg), "api")
        .await
        .expect("connect provider");
    ctx = ctx.with_runtime_config_provider(settings);

    let app = build_router(ctx);

    // 1) descriptors lists the registered key
    let response = app
        .clone()
        .oneshot(req("GET", "/admin/config/descriptors").with_admin())
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(
        body["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| group["id"] == "demo" && group["label"] == "Demo"),
        "descriptors should include demo group: {body:?}"
    );
    assert!(
        body["data"].as_array().unwrap().iter().any(|d| {
            d["key"] == "demo.flag"
                && d["group"] == "demo"
                && d["section"] == "Basics"
                && d["order"] == 10
                && d["visible_when"].is_null()
        }),
        "descriptors should include demo.flag: {body:?}"
    );
    assert!(
        body["data"].as_array().unwrap().iter().any(|d| {
            d["key"] == "demo.locked"
                && d["visible_when"]["kind"] == "equals"
                && d["visible_when"]["key"] == "demo.flag"
                && d["visible_when"]["value"] == true
        }),
        "descriptors should include demo.locked visibility: {body:?}"
    );

    // 2) unauthenticated request is rejected
    let response = app
        .clone()
        .oneshot(req("GET", "/admin/config/descriptors"))
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 3) valid write returns 200 with applies_on_restart=false
    let response = app
        .clone()
        .oneshot(req_json("PUT", "/admin/config/*/demo.flag", &json!({"value": true})).with_admin())
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["value"], json!(true));
    assert_eq!(body["applies_on_restart"], json!(false));

    // 4) invalid value type returns 400
    let response = app
        .clone()
        .oneshot(
            req_json(
                "PUT",
                "/admin/config/*/demo.flag",
                &json!({"value": "nope"}),
            )
            .with_admin(),
        )
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 5) unknown key returns 404
    let response = app
        .clone()
        .oneshot(
            req_json(
                "PUT",
                "/admin/config/*/unknown.key",
                &json!({"value": true}),
            )
            .with_admin(),
        )
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // 5b) writing a non-editable key returns 403
    let response = app
        .clone()
        .oneshot(
            req_json(
                "PUT",
                "/admin/config/*/demo.locked",
                &json!({"value": false}),
            )
            .with_admin(),
        )
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // 6) generated values initialize once when their trigger value is written
    let response = app
        .clone()
        .oneshot(
            req_json(
                "PUT",
                "/admin/config/*/demo.mode",
                &json!({"value": "secret"}),
            )
            .with_admin(),
        )
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(req("GET", "/admin/config/*/demo.secret/audit").with_admin())
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let secret_entries = body["data"].as_array().unwrap();
    assert_eq!(secret_entries.len(), 1, "secret should initialize once");
    assert_eq!(
        secret_entries[0]["new_value"]
            .as_str()
            .expect("generated secret is a string")
            .len(),
        64
    );
    let response = app
        .clone()
        .oneshot(req("GET", "/admin/config/values").with_admin())
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let mode_value = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["key"] == "demo.mode")
        .expect("demo.mode value exists");
    assert_eq!(mode_value["value"], json!("basic"));
    assert_eq!(mode_value["effective_value"], json!("basic"));
    assert_eq!(mode_value["desired_value"], json!("secret"));
    assert_eq!(mode_value["pending_restart"], json!(true));

    let response = app
        .clone()
        .oneshot(
            req_json(
                "PUT",
                "/admin/config/*/demo.mode",
                &json!({"value": "secret"}),
            )
            .with_admin(),
        )
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(req("GET", "/admin/config/*/demo.secret/audit").with_admin())
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        1,
        "secret should not be regenerated"
    );

    // 7) the audit endpoint reads the DB directly and should reflect the write
    let response = app
        .clone()
        .oneshot(req("GET", "/admin/config/*/demo.flag/audit").with_admin())
        .await
        .expect("request completes");
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let entries = body["data"].as_array().unwrap();
    assert!(!entries.is_empty(), "audit should have at least one entry");
    assert_eq!(entries[0]["new_value"], json!(true));

    db.cleanup().await;
}
