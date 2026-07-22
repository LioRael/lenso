//! System Plane production-delivery authority APIs.
//!
//! Mutating delivery control belongs here, outside the read-only Runtime
//! Console observability boundary in `platform-admin`.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use lenso_service::{ReleaseSignerStatus, ReleaseTrustProvider as _};
use platform_core::{AppContext, AppError, ErrorCode};
use platform_http::{
    AdminActor, ApiErrorResponse, ApiOpenApiRouter, ErrorResponse, HttpRequestContext,
    OpenApiRouter, routes,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

const DELIVERY_WRITE_CAPABILITY: &str = "runtime.deliveries.write";

#[allow(clippy::large_enum_variant)]
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum DeliveryArtifactSchema {
    ServiceRelease(lenso_service::ServiceRelease),
    ReleaseTrustEvidence(lenso_service::ReleaseTrustEvidence),
    PolicyEvidence(lenso_service::PolicyEvidence),
    ConfigRevision(lenso_service::ConfigRevision),
    ConfigActivationReceipt(lenso_service::ConfigActivationReceipt),
    EdgeContract(lenso_service::EdgeContract),
    GatewayPlan(lenso_service::GatewayConfigurationPlan),
    GatewayObservation(lenso_service::GatewayObservation),
    DeploymentPlan(lenso_service::DeploymentPlan),
    DeploymentReceipt(lenso_service::DeploymentReceipt),
    DeploymentObservation(lenso_service::DeploymentObservation),
    EnvironmentVerification(lenso_service::EnvironmentVerification),
    PromotionPlan(lenso_service::PromotionPlan),
    PromotionApproval(lenso_service::PromotionApproval),
    PromotionReceipt(lenso_service::PromotionReceipt),
    CanaryPlan(lenso_service::CanaryPlan),
    CanaryDecision(lenso_service::CanaryDecision),
    ReliabilityObservation(lenso_service::ReliabilityObservation),
    RollbackPlan(lenso_service::RollbackPlan),
    RollbackReceipt(lenso_service::RollbackReceipt),
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeliveryArtifactRecordRequest {
    provider_id: String,
    provider_proof: String,
    artifacts: Vec<DeliveryArtifactSchema>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeliveryArtifactRecordEffects {
    appends_ledger: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeliveryArtifactRecordResponse {
    protocol: String,
    delivery_id: String,
    batch_subject: String,
    provider_id: String,
    recorded: usize,
    effects: DeliveryArtifactRecordEffects,
}

#[utoipa::path(
    post,
    path = "/system/delivery/policy/evaluate",
    operation_id = "system_delivery_evaluate_policy",
    tag = "system-delivery",
    request_body = lenso_service::DeliveryPolicyInputs,
    responses(
        (status = 200, description = "Canonical System Plane Policy Evidence", body = lenso_service::PolicyEvidence),
        (status = 400, description = "Canonical policy input or authority configuration is invalid", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 403, description = "System Plane service authority is required", body = ErrorResponse, content_type = "application/problem+json")
    )
)]
async fn evaluate_delivery_policy(
    actor: AdminActor,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Json(inputs): Json<lenso_service::DeliveryPolicyInputs>,
) -> Result<Json<lenso_service::PolicyEvidence>, ApiErrorResponse> {
    ensure_service_or_system(&actor, &request_ctx)?;
    let trust_keys: BTreeMap<String, String> =
        parse_authority_env("LENSO_DELIVERY_TRUST_KEYS", &request_ctx)?;
    let secret_observations: BTreeMap<String, lenso_service::SecretReferenceObservation> =
        parse_authority_env("LENSO_DELIVERY_SECRET_OBSERVATIONS", &request_ctx)?;
    let secret_provider_name =
        std::env::var("LENSO_DELIVERY_SECRET_PROVIDER").map_err(|source| {
            ApiErrorResponse::with_context(
                AppError::validation("LENSO_DELIVERY_SECRET_PROVIDER is required", Vec::new())
                    .with_source(source),
                &request_ctx,
            )
        })?;
    let trust_provider = lenso_service::DeterministicTrustProvider::new(trust_keys);
    let secret_provider =
        lenso_service::DeterministicSecretProvider::new(secret_provider_name, secret_observations);
    Ok(Json(lenso_service::evaluate_delivery_policy(
        &lenso_service::production_policy_pack(),
        &inputs,
        &trust_provider,
        &secret_provider,
        lenso_service::PolicyEvaluationSurface::SystemPlane,
    )))
}

#[utoipa::path(
    post,
    path = "/system/delivery/deliveries/{delivery_id}/artifacts",
    operation_id = "system_delivery_record_artifacts",
    tag = "system-delivery",
    params(("delivery_id" = String, Path, description = "Stable production delivery identity")),
    request_body = DeliveryArtifactRecordRequest,
    responses(
        (status = 200, description = "Immutable delivery artifacts recorded", body = DeliveryArtifactRecordResponse),
        (status = 400, description = "Artifact is invalid or secret-shaped", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 401, description = "Authentication is required", body = ErrorResponse, content_type = "application/problem+json"),
        (status = 403, description = "System Plane service authority is required", body = ErrorResponse, content_type = "application/problem+json")
    )
)]
async fn record_delivery_artifacts(
    actor: AdminActor,
    State(ctx): State<AppContext>,
    Path(delivery_id): Path<String>,
    HttpRequestContext(request_ctx): HttpRequestContext,
    Json(body): Json<DeliveryArtifactRecordRequest>,
) -> Result<Json<DeliveryArtifactRecordResponse>, ApiErrorResponse> {
    ensure_delivery_write(&actor, &request_ctx)?;
    if delivery_id.trim().is_empty()
        || body.provider_id.trim().is_empty()
        || body.provider_proof.trim().is_empty()
        || body.artifacts.is_empty()
    {
        return Err(validation_error(
            "delivery artifact request is incomplete",
            &request_ctx,
        ));
    }
    let trust_keys: BTreeMap<String, String> =
        parse_authority_env("LENSO_DELIVERY_TRUST_KEYS", &request_ctx)?;
    let trust_provider = lenso_service::DeterministicTrustProvider::new(trust_keys);
    let artifacts = body
        .artifacts
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| {
            ApiErrorResponse::with_context(
                AppError::validation("delivery artifact batch is invalid", Vec::new())
                    .with_source(source),
                &request_ctx,
            )
        })?;
    let batch_subject = lenso_service::delivery_artifact_batch_subject(&delivery_id, &artifacts);
    if trust_provider.verify(&body.provider_id, &batch_subject, &body.provider_proof)
        != ReleaseSignerStatus::Trusted
    {
        return Err(validation_error(
            "delivery artifact batch proof is not trusted",
            &request_ctx,
        ));
    }
    let releases = typed_artifacts::<lenso_service::ServiceRelease>(
        &artifacts,
        lenso_service::SERVICE_RELEASE_PROTOCOL,
        "Service Release artifact is invalid",
        &request_ctx,
    )?;
    let trust_evidence = typed_artifacts::<lenso_service::ReleaseTrustEvidence>(
        &artifacts,
        lenso_service::RELEASE_TRUST_EVIDENCE_PROTOCOL,
        "Release Trust Evidence artifact is invalid",
        &request_ctx,
    )?;
    if releases.len() != 1
        || trust_evidence.len() != 1
        || releases[0].release_id != delivery_id
        || !lenso_service::release_trust_evidence_integrity_is_valid(
            &trust_evidence[0],
            &releases[0],
            &trust_provider,
        )
    {
        return Err(validation_error(
            "delivery batch must contain one matching trusted Service Release",
            &request_ctx,
        ));
    }
    lenso_service::record_delivery_artifacts(&ctx.db, &delivery_id, &artifacts)
        .await
        .map_err(|source| {
            ApiErrorResponse::with_context(
                AppError::validation("delivery artifact batch was rejected", Vec::new())
                    .with_source(source),
                &request_ctx,
            )
        })?;
    Ok(Json(DeliveryArtifactRecordResponse {
        protocol: "lenso.delivery-artifact-recording.v1".to_owned(),
        delivery_id,
        batch_subject,
        provider_id: body.provider_id,
        recorded: artifacts.len(),
        effects: DeliveryArtifactRecordEffects {
            appends_ledger: true,
        },
    }))
}

pub(crate) fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new()
        .routes(routes!(evaluate_delivery_policy))
        .routes(routes!(record_delivery_artifacts))
}

fn ensure_service_or_system(
    actor: &AdminActor,
    request_ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    match actor {
        AdminActor::System | AdminActor::Service { .. } => Ok(()),
        AdminActor::User { .. } => Err(forbidden_error(
            "Service or system authentication is required",
            request_ctx,
        )),
    }
}

fn ensure_delivery_write(
    actor: &AdminActor,
    request_ctx: &platform_core::RequestContext,
) -> Result<(), ApiErrorResponse> {
    match actor {
        AdminActor::System => Ok(()),
        AdminActor::Service { scopes, .. }
            if scopes
                .iter()
                .any(|scope| scope == DELIVERY_WRITE_CAPABILITY) =>
        {
            Ok(())
        }
        AdminActor::Service { .. } | AdminActor::User { .. } => Err(forbidden_error(
            format!("missing production delivery capability: {DELIVERY_WRITE_CAPABILITY}"),
            request_ctx,
        )),
    }
}

fn parse_authority_env<T: serde::de::DeserializeOwned>(
    name: &str,
    request_ctx: &platform_core::RequestContext,
) -> Result<T, ApiErrorResponse> {
    let value = std::env::var(name).map_err(|source| {
        ApiErrorResponse::with_context(
            AppError::validation(format!("{name} is required"), Vec::new()).with_source(source),
            request_ctx,
        )
    })?;
    serde_json::from_str(&value).map_err(|source| {
        ApiErrorResponse::with_context(
            AppError::validation(format!("{name} is invalid"), Vec::new()).with_source(source),
            request_ctx,
        )
    })
}

fn typed_artifacts<T: serde::de::DeserializeOwned>(
    artifacts: &[Value],
    protocol: &str,
    message: &str,
    request_ctx: &platform_core::RequestContext,
) -> Result<Vec<T>, ApiErrorResponse> {
    artifacts
        .iter()
        .filter(|artifact| artifact.get("protocol").and_then(Value::as_str) == Some(protocol))
        .map(|artifact| serde_json::from_value(artifact.clone()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| {
            ApiErrorResponse::with_context(
                AppError::validation(message, Vec::new()).with_source(source),
                request_ctx,
            )
        })
}

fn validation_error(
    message: impl Into<String>,
    request_ctx: &platform_core::RequestContext,
) -> ApiErrorResponse {
    ApiErrorResponse::with_context(AppError::validation(message, Vec::new()), request_ctx)
}

fn forbidden_error(
    message: impl Into<String>,
    request_ctx: &platform_core::RequestContext,
) -> ApiErrorResponse {
    ApiErrorResponse::with_context(AppError::new(ErrorCode::Forbidden, message), request_ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_request_uses_the_raw_openapi_wire_shape() {
        let release: Value = serde_json::from_str(include_str!(
            "../../../contracts/delivery/support.service-release.json"
        ))
        .expect("committed Service Release fixture should parse");
        let request: DeliveryArtifactRecordRequest = serde_json::from_value(serde_json::json!({
            "providerId": "ci:contract-test",
            "providerProof": "sha256:contract-test",
            "artifacts": [release.clone()],
        }))
        .expect("the real handler request must accept the raw Service Release object");

        assert_eq!(request.artifacts.len(), 1);
        assert_eq!(
            serde_json::to_value(&request.artifacts[0])
                .expect("typed handler artifact should serialize"),
            release,
            "handler wire format must not introduce an enum variant wrapper"
        );
    }
}
