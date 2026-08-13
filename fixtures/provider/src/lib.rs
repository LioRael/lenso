pub mod grpc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use platform_module::ModuleManifest;
use platform_provider::{
    PROVIDER_PROTOCOL, ProviderDescriptor, ProviderErrorBody, ProviderExport, ProviderExportHealth,
    ProviderHealth, ProviderHostEffectBatch, ProviderInvocation, ProviderInvocationAcknowledgement,
    ProviderInvocationReference, ProviderOperationKind, ProviderOutcome, ProviderOutcomeStatus,
    ProviderTransportBinding,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::sync::Mutex;

const SERVICE_DIGEST: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MODULE_DIGEST: &str =
    "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const MANIFEST_DIGEST: &str =
    "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const CONTRACT_DIGEST: &str =
    "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

#[derive(Debug, Clone)]
pub struct ProviderFixtureState {
    inner: Arc<Mutex<FixtureStore>>,
}

#[derive(Debug, Default)]
struct FixtureStore {
    invocations: HashMap<String, StoredInvocation>,
}

#[derive(Debug, Clone)]
struct StoredInvocation {
    request_digest: String,
    outcome: ProviderOutcome,
    acknowledged: bool,
}

impl Default for ProviderFixtureState {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FixtureStore::default())),
        }
    }
}

pub fn app() -> Router {
    app_with_state(ProviderFixtureState::default())
}

pub fn app_with_state(state: ProviderFixtureState) -> Router {
    Router::new()
        .route("/lenso/provider/v1", get(describe_provider))
        .route("/lenso/provider/v1/health/live", get(health))
        .route("/lenso/provider/v1/health/ready", get(health))
        .route(
            "/lenso/provider/v1/exports/{export_key}/http:invoke",
            post(invoke_http),
        )
        .route(
            "/lenso/provider/v1/exports/{export_key}/admin:list",
            post(invoke_admin_list),
        )
        .route(
            "/lenso/provider/v1/exports/{export_key}/admin:get",
            post(invoke_admin_get),
        )
        .route(
            "/lenso/provider/v1/exports/{export_key}/admin:query",
            post(invoke_admin_query),
        )
        .route(
            "/lenso/provider/v1/exports/{export_key}/admin:act",
            post(invoke_admin_action),
        )
        .route(
            "/lenso/provider/v1/exports/{export_key}/runtime:invoke",
            post(invoke_runtime),
        )
        .route(
            "/lenso/provider/v1/exports/{export_key}/events:handle",
            post(invoke_event),
        )
        .route(
            "/lenso/provider/v1/invocations/{*invocation_id}",
            get(get_invocation).post(acknowledge_invocation),
        )
        .with_state(state)
}

async fn describe_provider() -> Json<ProviderDescriptor> {
    Json(descriptor())
}

async fn health() -> Json<ProviderHealth> {
    Json(ProviderHealth {
        protocol: PROVIDER_PROTOCOL.to_owned(),
        service_id: "fixture-provider".to_owned(),
        service_release_digest: SERVICE_DIGEST.to_owned(),
        live: true,
        ready: true,
        observed_at: "2026-07-30T00:00:00Z".to_owned(),
        exports: ["contacts", "billing"]
            .into_iter()
            .map(|export| {
                (
                    export.to_owned(),
                    ProviderExportHealth {
                        ready: true,
                        reasons: Vec::new(),
                    },
                )
            })
            .into_iter()
            .collect(),
    })
}

macro_rules! invocation_handler {
    ($name:ident, $kind:expr) => {
        async fn $name(
            State(state): State<ProviderFixtureState>,
            Path(export_key): Path<String>,
            Json(invocation): Json<ProviderInvocation>,
        ) -> Result<Json<ProviderOutcome>, (StatusCode, Json<Value>)> {
            state.invoke(&export_key, $kind, invocation).await.map(Json)
        }
    };
}

invocation_handler!(invoke_http, ProviderOperationKind::HttpRoute);
invocation_handler!(invoke_admin_list, ProviderOperationKind::AdminList);
invocation_handler!(invoke_admin_get, ProviderOperationKind::AdminGet);
invocation_handler!(invoke_admin_query, ProviderOperationKind::AdminQuery);
invocation_handler!(invoke_admin_action, ProviderOperationKind::AdminAction);
invocation_handler!(invoke_runtime, ProviderOperationKind::RuntimeFunction);
invocation_handler!(invoke_event, ProviderOperationKind::EventHandler);

async fn get_invocation(
    State(state): State<ProviderFixtureState>,
    Path(invocation_id): Path<String>,
) -> Result<Json<ProviderOutcome>, StatusCode> {
    state
        .get(&invocation_id)
        .await
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn acknowledge_invocation(
    State(state): State<ProviderFixtureState>,
    Path(invocation_id): Path<String>,
    Json(ack): Json<ProviderInvocationAcknowledgement>,
) -> Result<Json<ProviderInvocationReference>, StatusCode> {
    let invocation_id = invocation_id
        .strip_suffix(":ack")
        .ok_or(StatusCode::NOT_FOUND)?;
    state.acknowledge(invocation_id, &ack).await.map(Json)
}

impl ProviderFixtureState {
    pub async fn get(&self, invocation_id: &str) -> Option<ProviderOutcome> {
        self.inner
            .lock()
            .await
            .invocations
            .get(invocation_id)
            .map(|stored| stored.outcome.clone())
    }

    pub async fn acknowledge(
        &self,
        invocation_id: &str,
        ack: &ProviderInvocationAcknowledgement,
    ) -> Result<ProviderInvocationReference, StatusCode> {
        let mut store = self.inner.lock().await;
        let stored = store
            .invocations
            .get_mut(invocation_id)
            .ok_or(StatusCode::NOT_FOUND)?;
        if ack.invocation_id != invocation_id || ack.outcome_digest != stored.outcome.outcome_digest
        {
            return Err(StatusCode::CONFLICT);
        }
        stored.acknowledged = true;
        Ok(ProviderInvocationReference {
            invocation_id: invocation_id.to_owned(),
        })
    }

    pub async fn invoke(
        &self,
        export_key: &str,
        expected_kind: ProviderOperationKind,
        invocation: ProviderInvocation,
    ) -> Result<ProviderOutcome, (StatusCode, Json<Value>)> {
        validate_invocation(export_key, &expected_kind, &invocation)?;
        let request_digest = digest(&invocation);
        let mut store = self.inner.lock().await;
        if let Some(stored) = store.invocations.get(&invocation.invocation_id) {
            if stored.request_digest != request_digest {
                return Err(problem(
                    StatusCode::CONFLICT,
                    "invocation_identity_conflict",
                    "invocation id was already bound to a different request",
                ));
            }
            return Ok(stored.outcome.clone());
        }
        let result = match expected_kind {
            ProviderOperationKind::HttpRoute => json!({
                "status_code": 200,
                "body": invocation.payload,
            }),
            ProviderOperationKind::AdminList => json!({
                "records": [],
                "next_cursor": null,
            }),
            ProviderOperationKind::AdminGet => json!({ "record": null }),
            ProviderOperationKind::AdminQuery => json!({ "data": invocation.payload }),
            ProviderOperationKind::AdminAction => json!({ "result": invocation.payload }),
            ProviderOperationKind::RuntimeFunction => json!({ "output": invocation.payload }),
            ProviderOperationKind::EventHandler => json!({ "actions": [] }),
        };
        let mut outcome = ProviderOutcome {
            protocol: PROVIDER_PROTOCOL.to_owned(),
            invocation_id: invocation.invocation_id.clone(),
            status: ProviderOutcomeStatus::Succeeded,
            result: Some(result),
            error: None,
            effect_evidence: vec![json!({ "effect": "fixture_committed" })],
            host_effects: ProviderHostEffectBatch::default(),
            outcome_digest: String::new(),
        };
        outcome.outcome_digest = digest(&outcome);
        store.invocations.insert(
            invocation.invocation_id,
            StoredInvocation {
                request_digest,
                outcome: outcome.clone(),
                acknowledged: false,
            },
        );
        Ok(outcome)
    }
}

fn validate_invocation(
    export_key: &str,
    expected_kind: &ProviderOperationKind,
    invocation: &ProviderInvocation,
) -> Result<(), (StatusCode, Json<Value>)> {
    if invocation.protocol != PROVIDER_PROTOCOL
        || export_key != "contacts"
        || invocation.export_key != export_key
        || invocation.service_release_digest != SERVICE_DIGEST
        || invocation.module_release_digest != MODULE_DIGEST
        || invocation.manifest_digest != MANIFEST_DIGEST
        || std::mem::discriminant(&invocation.operation_kind)
            != std::mem::discriminant(expected_kind)
        || invocation.input_contract_digest != CONTRACT_DIGEST
        || invocation.output_contract_digest != CONTRACT_DIGEST
    {
        return Err(problem(
            StatusCode::PRECONDITION_FAILED,
            "provider_contract_mismatch",
            "invocation does not match the installed Provider Export",
        ));
    }
    Ok(())
}

pub fn descriptor() -> ProviderDescriptor {
    let manifest = ModuleManifest::builder("fixture/contacts").build();
    let billing_manifest = ModuleManifest::builder("fixture/billing").build();
    ProviderDescriptor {
        protocol: PROVIDER_PROTOCOL.to_owned(),
        protocol_contract_digest: CONTRACT_DIGEST.to_owned(),
        service_id: "fixture-provider".to_owned(),
        service_release_version: "1.0.0".to_owned(),
        service_release_digest: SERVICE_DIGEST.to_owned(),
        runtime_instance_id: "fixture-provider-instance".to_owned(),
        features: vec!["durable_invocations".to_owned()],
        transports: vec![
            ProviderTransportBinding::HttpJson,
            ProviderTransportBinding::Grpc,
        ],
        exports: vec![
            ProviderExport {
                export_key: "contacts".to_owned(),
                module_id: manifest.module_id.clone(),
                module_version: "1.0.0".to_owned(),
                module_release_digest: MODULE_DIGEST.to_owned(),
                manifest_digest: MANIFEST_DIGEST.to_owned(),
                manifest,
                contract_digests: [("contacts".to_owned(), CONTRACT_DIGEST.to_owned())]
                    .into_iter()
                    .collect::<BTreeMap<_, _>>(),
                ready: true,
                readiness_reasons: Vec::new(),
            },
            ProviderExport {
                export_key: "billing".to_owned(),
                module_id: billing_manifest.module_id.clone(),
                module_version: "1.0.0".to_owned(),
                module_release_digest:
                    "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                        .to_owned(),
                manifest_digest:
                    "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                        .to_owned(),
                manifest: billing_manifest,
                contract_digests: [("billing".to_owned(), CONTRACT_DIGEST.to_owned())]
                    .into_iter()
                    .collect::<BTreeMap<_, _>>(),
                ready: true,
                readiness_reasons: Vec::new(),
            },
        ],
    }
}

fn digest(value: &impl Serialize) -> String {
    let bytes = serde_json_canonicalizer::to_vec(value).expect("fixture value canonicalizes");
    let encoded = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{encoded}")
}

fn problem(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<Value>) {
    let error = ProviderErrorBody {
        code: code.to_owned(),
        message: message.to_owned(),
        retryable: false,
        retry_after_ms: None,
        provider_trace_reference: None,
        details: Vec::new(),
    };
    (status, Json(json!({ "error": error })))
}
