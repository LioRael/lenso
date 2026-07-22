use std::collections::{BTreeMap, BTreeSet};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use http::Uri;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::extraction_input_digest;

use super::{
    DeliveryEffects, DeliveryIssue, DeliveryIssueCode, ReleaseSignerStatus, ReleaseTrustProvider,
    ServiceRelease, issue, service_release_integrity_is_valid,
};

pub const EDGE_CONTRACT_PROTOCOL: &str = "lenso.edge-contract.v1";
pub const GATEWAY_PLAN_PROTOCOL: &str = "lenso.gateway-plan.v1";
pub const GATEWAY_OBSERVATION_PROTOCOL: &str = "lenso.gateway-observation.v1";

pub trait GatewayObservationProvider: std::fmt::Debug + Send + Sync {
    fn provider_id(&self) -> &str;

    fn sign(&self, observation_id: &str) -> Option<String>;

    fn verify(&self, observation_id: &str, proof: &str) -> bool;
}

#[derive(Debug, Clone)]
pub struct DeterministicGatewayObservationProvider {
    provider_id: String,
    key: String,
}

impl DeterministicGatewayObservationProvider {
    #[must_use]
    pub fn new(provider_id: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            key: key.into(),
        }
    }

    fn expected_proof(&self, observation_id: &str) -> String {
        digest_json(&(
            "lenso.gateway-observation-authority-proof.v1",
            self.provider_id.as_str(),
            observation_id,
            self.key.as_str(),
        ))
    }
}

impl GatewayObservationProvider for DeterministicGatewayObservationProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn sign(&self, observation_id: &str) -> Option<String> {
        Some(self.expected_proof(observation_id))
    }

    fn verify(&self, observation_id: &str, proof: &str) -> bool {
        self.expected_proof(observation_id) == proof
    }
}

/// Verify-only Gateway observation authority backed by an Ed25519 public key.
#[derive(Debug, Clone)]
pub struct Ed25519GatewayObservationProvider {
    provider_id: String,
    public_key: VerifyingKey,
    signing_key: Option<SigningKey>,
}

impl Ed25519GatewayObservationProvider {
    pub fn from_base64_public_key(
        provider_id: impl Into<String>,
        encoded: &str,
    ) -> Result<Self, String> {
        let bytes = BASE64
            .decode(encoded)
            .map_err(|error| format!("invalid Ed25519 public key encoding: {error}"))?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "Ed25519 public keys must contain exactly 32 bytes".to_owned())?;
        let public_key = VerifyingKey::from_bytes(&bytes)
            .map_err(|error| format!("invalid Ed25519 public key: {error}"))?;
        Ok(Self {
            provider_id: provider_id.into(),
            public_key,
            signing_key: None,
        })
    }

    pub fn from_base64_private_key(
        provider_id: impl Into<String>,
        encoded: &str,
    ) -> Result<Self, String> {
        let bytes = BASE64
            .decode(encoded)
            .map_err(|error| format!("invalid Ed25519 private key encoding: {error}"))?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "Ed25519 private keys must contain exactly 32 bytes".to_owned())?;
        let signing_key = SigningKey::from_bytes(&bytes);
        Ok(Self {
            provider_id: provider_id.into(),
            public_key: signing_key.verifying_key(),
            signing_key: Some(signing_key),
        })
    }
}

impl GatewayObservationProvider for Ed25519GatewayObservationProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn sign(&self, observation_id: &str) -> Option<String> {
        self.signing_key
            .as_ref()
            .map(|key| BASE64.encode(key.sign(observation_id.as_bytes()).to_bytes()))
    }

    fn verify(&self, observation_id: &str, proof: &str) -> bool {
        let Ok(bytes) = BASE64.decode(proof) else {
            return false;
        };
        let Ok(signature) = Signature::from_slice(&bytes) else {
            return false;
        };
        self.public_key
            .verify(observation_id.as_bytes(), &signature)
            .is_ok()
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EdgeOperationVisibility {
    PublicEligible,
    Internal,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct EdgeServiceOperation {
    pub contract_id: String,
    pub contract_version: String,
    pub contract_digest: String,
    pub operation_id: String,
    pub visibility: EdgeOperationVisibility,
    pub request_schema_reference: String,
    pub response_schema_reference: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum EdgeAuthentication {
    Public,
    Workload,
    User,
    WorkloadOrUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorsIntent {
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    #[serde(default)]
    pub allowed_methods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RateIntent {
    pub requests: u32,
    pub window_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EdgeRoute {
    pub contract_id: String,
    pub contract_version: String,
    pub operation_id: String,
    pub public_path: String,
    pub authentication: EdgeAuthentication,
    pub cors: CorsIntent,
    pub rate: RateIntent,
    pub deprecated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedEdgeRoute {
    pub contract_id: String,
    pub contract_version: String,
    pub operation_id: String,
    pub public_path: String,
    pub authentication: EdgeAuthentication,
    pub cors: CorsIntent,
    pub rate: RateIntent,
    pub deprecated: bool,
    pub request_schema_reference: String,
    pub response_schema_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EdgeContract {
    pub protocol: String,
    pub edge_contract_id: String,
    pub edge_contract_digest: String,
    pub service_id: String,
    pub release_id: String,
    pub release_digest: String,
    pub operation_catalog_digest: String,
    pub provider_id: String,
    pub provider_proof: String,
    pub routes: Vec<ResolvedEdgeRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatewayEnvironmentBinding {
    pub environment: String,
    pub gateway_adapter: String,
    pub public_origin: String,
    pub expected_gateway_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatewayObservation {
    pub protocol: String,
    pub observation_id: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub environment: String,
    pub release_id: String,
    pub release_digest: String,
    pub resource_uid: String,
    pub resource_version: String,
    pub authority_context: String,
    pub configuration_identity: String,
    pub revision: u64,
    pub observed_after: String,
    pub fresh: bool,
    pub provider_id: String,
    pub provider_proof: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatewayPlanDiffEntry {
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GatewayConfigurationPlan {
    pub protocol: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub edge_contract_id: String,
    pub edge_contract_digest: String,
    pub edge_release_id: String,
    pub edge_release_digest: String,
    pub operation_catalog_digest: String,
    pub edge_provider_id: String,
    pub edge_provider_proof: String,
    pub environment: String,
    pub gateway_adapter: String,
    pub public_origin: String,
    pub expected_gateway_revision: u64,
    pub configuration_identity: String,
    pub routes: Vec<ResolvedEdgeRoute>,
    pub diff: Vec<GatewayPlanDiffEntry>,
    pub drifted: bool,
    pub issues: Vec<DeliveryIssue>,
    pub next_actions: Vec<String>,
    pub effects: DeliveryEffects,
}

pub fn build_edge_contract(
    release: &ServiceRelease,
    available_operations: &[EdgeServiceOperation],
    provider_id: &str,
    provider: &dyn ReleaseTrustProvider,
    mut routes: Vec<EdgeRoute>,
) -> Result<EdgeContract, Vec<DeliveryIssue>> {
    let service_id = release.service_id.clone();
    let mut canonical_operations = available_operations.to_vec();
    canonical_operations.sort();
    let operation_catalog_digest = digest_json(&(
        "lenso.service-operation-catalog.v1",
        release.release_id.as_str(),
        release.release_digest.as_str(),
        canonical_operations.as_slice(),
    ));
    routes.sort_by(|left, right| {
        (
            &left.public_path,
            &left.operation_id,
            &left.contract_version,
        )
            .cmp(&(
                &right.public_path,
                &right.operation_id,
                &right.contract_version,
            ))
    });
    let operations = available_operations
        .iter()
        .map(|operation| {
            (
                (
                    operation.contract_id.as_str(),
                    operation.contract_version.as_str(),
                    operation.operation_id.as_str(),
                ),
                operation,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut issues = Vec::new();
    let operation_keys = available_operations
        .iter()
        .map(|operation| {
            (
                operation.contract_id.as_str(),
                operation.contract_version.as_str(),
                operation.operation_id.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    if !service_release_integrity_is_valid(release)
        || provider_id.trim().is_empty()
        || operation_keys.len() != available_operations.len()
        || available_operations.is_empty()
        || available_operations.iter().any(|operation| {
            !release.contract_versions.iter().any(|contract| {
                contract.contract_id == operation.contract_id
                    && contract.version == operation.contract_version
                    && contract.artifact.digest == operation.contract_digest
            })
        })
    {
        issues.push(issue(
            DeliveryIssueCode::EdgeOperationUnknown,
            "The Service operation catalog is not a trusted, release-bound projection of its exact Contract artifacts.",
            "Project all operations from the immutable Service Contract set through a trusted provider.",
            "Refresh the operation catalog for the exact Service Release.",
        ));
    }
    let mut paths = BTreeSet::new();
    let mut resolved = Vec::new();
    for route in routes {
        if !public_path_template_is_safe(&route.public_path)
            || route.rate.requests == 0
            || route.rate.window_seconds == 0
            || !cors_intent_is_safe(&route.cors)
        {
            issues.push(issue(
                DeliveryIssueCode::EdgeExposureUnsafe,
                format!(
                    "Edge route `{}` has an invalid public path, CORS intent, or rate intent.",
                    route.operation_id
                ),
                "Declare an absolute path, HTTP(S) origins, standard uppercase methods, and a positive bounded rate intent.",
                "Correct the Edge route and generate it again.",
            ));
            continue;
        }
        if !paths.insert(route.public_path.clone()) {
            issues.push(issue(
                DeliveryIssueCode::EdgePathConflict,
                format!(
                    "Public path `{}` is declared more than once.",
                    route.public_path
                ),
                "Assign one explicit Service Contract operation to each public path.",
                "Resolve the path conflict and generate the Edge Contract again.",
            ));
            continue;
        }
        let key = (
            route.contract_id.as_str(),
            route.contract_version.as_str(),
            route.operation_id.as_str(),
        );
        let Some(operation) = operations.get(&key) else {
            issues.push(issue(
                DeliveryIssueCode::EdgeOperationUnknown,
                format!(
                    "Edge route `{}` does not reference an exact Service Contract operation.",
                    route.operation_id
                ),
                "Reference an operation and version from the authoritative Service Contract.",
                "Correct the operation reference and generate the Edge Contract again.",
            ));
            continue;
        };
        if operation.visibility != EdgeOperationVisibility::PublicEligible {
            issues.push(issue(
                DeliveryIssueCode::EdgeExposureUnsafe,
                format!("Operation `{}` is internal and cannot be exposed.", route.operation_id),
                "Keep administration, Story feeds, health internals, and Workload management private.",
                "Select an explicitly public-eligible Service Contract operation.",
            ));
            continue;
        }
        let request_schema_reference = operation.request_schema_reference.clone();
        let response_schema_reference = operation.response_schema_reference.clone();
        resolved.push(ResolvedEdgeRoute {
            contract_id: route.contract_id,
            contract_version: route.contract_version,
            operation_id: route.operation_id,
            public_path: route.public_path,
            authentication: route.authentication,
            cors: route.cors,
            rate: route.rate,
            deprecated: route.deprecated,
            request_schema_reference,
            response_schema_reference,
        });
    }
    if service_id.trim().is_empty() || resolved.is_empty() {
        issues.push(issue(
            DeliveryIssueCode::EdgeExposureUnsafe,
            "An Edge Contract requires a Service identity and at least one valid public operation.",
            "Declare only intended public operations from an authoritative Service Contract.",
            "Correct the Edge Contract input and generate it again.",
        ));
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    let authority_subject = edge_authority_subject(
        release.release_id.as_str(),
        release.release_digest.as_str(),
        operation_catalog_digest.as_str(),
        resolved.as_slice(),
    );
    let Some(provider_proof) = provider.sign(provider_id, &authority_subject) else {
        return Err(vec![issue(
            DeliveryIssueCode::EdgeOperationUnknown,
            "The selected Edge authority provider is not trusted for the resolved public routes.",
            "Use a configured provider that attests the exact release, operation catalog, and resolved routes.",
            "Configure the provider and generate the Edge Contract again.",
        )]);
    };
    let edge_contract_digest = digest_json(&(
        EDGE_CONTRACT_PROTOCOL,
        service_id.as_str(),
        release.release_id.as_str(),
        release.release_digest.as_str(),
        operation_catalog_digest.as_str(),
        provider_id,
        provider_proof.as_str(),
        resolved.as_slice(),
    ));
    Ok(EdgeContract {
        protocol: EDGE_CONTRACT_PROTOCOL.to_owned(),
        edge_contract_id: format!("edge-contract:{edge_contract_digest}"),
        edge_contract_digest,
        service_id,
        release_id: release.release_id.clone(),
        release_digest: release.release_digest.clone(),
        operation_catalog_digest,
        provider_id: provider_id.to_owned(),
        provider_proof,
        routes: resolved,
    })
}

pub fn plan_gateway_configuration(
    edge: &EdgeContract,
    provider: &dyn ReleaseTrustProvider,
    binding: &GatewayEnvironmentBinding,
    observed: Option<&GatewayObservation>,
    observation_provider: &dyn GatewayObservationProvider,
) -> Result<GatewayConfigurationPlan, Vec<DeliveryIssue>> {
    if !edge_contract_authority_is_valid(edge, provider)
        || binding.environment.trim().is_empty()
        || binding.gateway_adapter.trim().is_empty()
        || !cors_origin_is_safe(&binding.public_origin)
        || observed.is_some_and(|observation| {
            !gateway_observation_integrity_is_valid(observation, observation_provider)
        })
    {
        return Err(vec![issue(
            DeliveryIssueCode::EdgeExposureUnsafe,
            "Gateway planning requires an integrity-valid Edge Contract and explicit environment binding.",
            "Correct the Edge Contract, adapter identity, and public origin.",
            "Regenerate the Gateway plan before mutation.",
        )]);
    }
    let configuration_identity = digest_json(&(
        edge.edge_contract_digest.as_str(),
        edge.release_id.as_str(),
        edge.release_digest.as_str(),
        edge.operation_catalog_digest.as_str(),
        edge.provider_id.as_str(),
        edge.provider_proof.as_str(),
        binding.environment.as_str(),
        binding.gateway_adapter.as_str(),
        binding.public_origin.as_str(),
        edge.routes.as_slice(),
    ));
    let drifted = observed.is_some_and(|observation| {
        observation.configuration_identity != configuration_identity
            || observation.revision != binding.expected_gateway_revision
            || !observation.fresh
    });
    let diff = observed
        .filter(|observation| observation.configuration_identity != configuration_identity)
        .map(|observation| {
            vec![GatewayPlanDiffEntry {
                subject: "gateway.configurationIdentity".to_owned(),
                before: Some(observation.configuration_identity.clone()),
                after: Some(configuration_identity.clone()),
            }]
        })
        .unwrap_or_default();
    let issues = Vec::new();
    let next_actions =
        vec!["Review and apply this plan through the selected Gateway Adapter.".to_owned()];
    let effects = DeliveryEffects::default();
    let plan_digest = digest_json(&(
        GATEWAY_PLAN_PROTOCOL,
        edge.edge_contract_id.as_str(),
        edge.edge_contract_digest.as_str(),
        edge.release_id.as_str(),
        edge.release_digest.as_str(),
        edge.operation_catalog_digest.as_str(),
        edge.provider_id.as_str(),
        edge.provider_proof.as_str(),
        binding,
        configuration_identity.as_str(),
        edge.routes.as_slice(),
        diff.as_slice(),
        drifted,
        issues.as_slice(),
        next_actions.as_slice(),
        &effects,
    ));
    Ok(GatewayConfigurationPlan {
        protocol: GATEWAY_PLAN_PROTOCOL.to_owned(),
        plan_id: format!("gateway-plan:{plan_digest}"),
        plan_digest,
        edge_contract_id: edge.edge_contract_id.clone(),
        edge_contract_digest: edge.edge_contract_digest.clone(),
        edge_release_id: edge.release_id.clone(),
        edge_release_digest: edge.release_digest.clone(),
        operation_catalog_digest: edge.operation_catalog_digest.clone(),
        edge_provider_id: edge.provider_id.clone(),
        edge_provider_proof: edge.provider_proof.clone(),
        environment: binding.environment.clone(),
        gateway_adapter: binding.gateway_adapter.clone(),
        public_origin: binding.public_origin.clone(),
        expected_gateway_revision: binding.expected_gateway_revision,
        configuration_identity,
        routes: edge.routes.clone(),
        diff,
        drifted,
        issues,
        next_actions,
        effects,
    })
}

#[must_use]
pub fn edge_contract_integrity_is_valid(edge: &EdgeContract) -> bool {
    edge.protocol == EDGE_CONTRACT_PROTOCOL
        && edge.routes.iter().all(resolved_edge_route_is_safe)
        && edge.edge_contract_id == format!("edge-contract:{}", edge.edge_contract_digest)
        && digest_json(&(
            edge.protocol.as_str(),
            edge.service_id.as_str(),
            edge.release_id.as_str(),
            edge.release_digest.as_str(),
            edge.operation_catalog_digest.as_str(),
            edge.provider_id.as_str(),
            edge.provider_proof.as_str(),
            edge.routes.as_slice(),
        )) == edge.edge_contract_digest
}

#[must_use]
pub fn edge_contract_authority_is_valid(
    edge: &EdgeContract,
    provider: &dyn ReleaseTrustProvider,
) -> bool {
    edge_contract_integrity_is_valid(edge)
        && provider.verify(
            edge.provider_id.as_str(),
            edge_authority_subject(
                edge.release_id.as_str(),
                edge.release_digest.as_str(),
                edge.operation_catalog_digest.as_str(),
                edge.routes.as_slice(),
            )
            .as_str(),
            edge.provider_proof.as_str(),
        ) == ReleaseSignerStatus::Trusted
}

#[must_use]
pub fn gateway_plan_integrity_is_valid(plan: &GatewayConfigurationPlan) -> bool {
    let binding = GatewayEnvironmentBinding {
        environment: plan.environment.clone(),
        gateway_adapter: plan.gateway_adapter.clone(),
        public_origin: plan.public_origin.clone(),
        expected_gateway_revision: plan.expected_gateway_revision,
    };
    plan.protocol == GATEWAY_PLAN_PROTOCOL
        && cors_origin_is_safe(&plan.public_origin)
        && plan.routes.iter().all(resolved_edge_route_is_safe)
        && plan.plan_id == format!("gateway-plan:{}", plan.plan_digest)
        && digest_json(&(
            plan.protocol.as_str(),
            plan.edge_contract_id.as_str(),
            plan.edge_contract_digest.as_str(),
            plan.edge_release_id.as_str(),
            plan.edge_release_digest.as_str(),
            plan.operation_catalog_digest.as_str(),
            plan.edge_provider_id.as_str(),
            plan.edge_provider_proof.as_str(),
            &binding,
            plan.configuration_identity.as_str(),
            plan.routes.as_slice(),
            plan.diff.as_slice(),
            plan.drifted,
            plan.issues.as_slice(),
            plan.next_actions.as_slice(),
            &plan.effects,
        )) == plan.plan_digest
}

fn resolved_edge_route_is_safe(route: &ResolvedEdgeRoute) -> bool {
    public_path_template_is_safe(&route.public_path)
        && route.rate.requests > 0
        && route.rate.window_seconds > 0
        && cors_intent_is_safe(&route.cors)
}

fn public_path_template_is_safe(path: &str) -> bool {
    if path.len() > 2_048 || !path.starts_with('/') || path == "/" || path.ends_with('/') {
        return false;
    }
    let mut parameters = BTreeSet::new();
    path[1..].split('/').all(|segment| {
        if segment.is_empty() {
            return false;
        }
        if let Some(parameter) = segment
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
        {
            !parameter.is_empty()
                && parameter.len() <= 64
                && parameter
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
                && parameter
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_alphabetic())
                && parameters.insert(parameter)
        } else {
            segment.len() <= 128
                && segment.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
                })
        }
    })
}

fn cors_intent_is_safe(cors: &CorsIntent) -> bool {
    let origins = cors.allowed_origins.iter().collect::<BTreeSet<_>>();
    let methods = cors.allowed_methods.iter().collect::<BTreeSet<_>>();
    origins.len() == cors.allowed_origins.len()
        && methods.len() == cors.allowed_methods.len()
        && cors.allowed_origins.len() <= 64
        && cors.allowed_methods.len() <= 7
        && (cors.allowed_origins.is_empty() == cors.allowed_methods.is_empty())
        && cors
            .allowed_origins
            .iter()
            .all(|origin| cors_origin_is_safe(origin))
        && cors
            .allowed_methods
            .iter()
            .all(|method| cors_method_is_safe(method))
}

fn cors_origin_is_safe(origin: &str) -> bool {
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    origin.len() <= 2_048
        && !origin.contains(['@', '"', '\\', '?', '#'])
        && !origin.chars().any(char::is_whitespace)
        && matches!(uri.scheme_str(), Some("http" | "https"))
        && uri.authority().is_some()
        && matches!(uri.path(), "" | "/")
        && uri.query().is_none()
}

fn cors_method_is_safe(method: &str) -> bool {
    matches!(
        method,
        "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
    )
}

#[must_use]
pub fn gateway_plan_authority_is_valid(
    plan: &GatewayConfigurationPlan,
    provider: &dyn ReleaseTrustProvider,
) -> bool {
    gateway_plan_integrity_is_valid(plan)
        && provider.verify(
            plan.edge_provider_id.as_str(),
            edge_authority_subject(
                plan.edge_release_id.as_str(),
                plan.edge_release_digest.as_str(),
                plan.operation_catalog_digest.as_str(),
                plan.routes.as_slice(),
            )
            .as_str(),
            plan.edge_provider_proof.as_str(),
        ) == ReleaseSignerStatus::Trusted
}

#[must_use]
pub fn edge_authority_subject(
    release_id: &str,
    release_digest: &str,
    operation_catalog_digest: &str,
    routes: &[ResolvedEdgeRoute],
) -> String {
    digest_json(&(
        "lenso.edge-authority-subject.v1",
        release_id,
        release_digest,
        operation_catalog_digest,
        routes,
    ))
}

#[must_use]
pub fn observe_gateway(
    plan: &GatewayConfigurationPlan,
    revision: u64,
    observed_after: impl Into<String>,
    fresh: bool,
    provider: &dyn GatewayObservationProvider,
) -> Result<GatewayObservation, DeliveryIssue> {
    let observed_after = observed_after.into();
    let fresh = fresh && revision == plan.expected_gateway_revision;
    let resource_uid = format!("synthetic:{}", plan.plan_id);
    let resource_version = "1";
    let authority_context = plan.plan_id.as_str();
    let digest = digest_json(&(
        GATEWAY_OBSERVATION_PROTOCOL,
        plan.plan_id.as_str(),
        plan.plan_digest.as_str(),
        plan.environment.as_str(),
        plan.edge_release_id.as_str(),
        plan.edge_release_digest.as_str(),
        resource_uid.as_str(),
        resource_version,
        authority_context,
        plan.configuration_identity.as_str(),
        revision,
        observed_after.as_str(),
        fresh,
        provider.provider_id(),
    ));
    let observation_id = format!("gateway-observation:{digest}");
    let provider_proof = provider.sign(&observation_id).ok_or_else(|| {
        issue(
            DeliveryIssueCode::ObservationStale,
            "The Gateway adapter authority refused to attest the observation.",
            "Use the configured Gateway observation provider at the adapter read boundary.",
            "Collect a new Gateway observation before continuing.",
        )
    })?;
    Ok(GatewayObservation {
        protocol: GATEWAY_OBSERVATION_PROTOCOL.to_owned(),
        observation_id,
        plan_id: plan.plan_id.clone(),
        plan_digest: plan.plan_digest.clone(),
        environment: plan.environment.clone(),
        release_id: plan.edge_release_id.clone(),
        release_digest: plan.edge_release_digest.clone(),
        resource_uid,
        resource_version: resource_version.to_owned(),
        authority_context: authority_context.to_owned(),
        configuration_identity: plan.configuration_identity.clone(),
        revision,
        observed_after,
        fresh,
        provider_id: provider.provider_id().to_owned(),
        provider_proof,
    })
}

#[must_use]
pub fn gateway_observation_content_integrity_is_valid(observation: &GatewayObservation) -> bool {
    observation.protocol == GATEWAY_OBSERVATION_PROTOCOL
        && !observation.provider_id.trim().is_empty()
        && !observation.provider_proof.trim().is_empty()
        && observation.observation_id
            == format!(
                "gateway-observation:{}",
                digest_json(&(
                    observation.protocol.as_str(),
                    observation.plan_id.as_str(),
                    observation.plan_digest.as_str(),
                    observation.environment.as_str(),
                    observation.release_id.as_str(),
                    observation.release_digest.as_str(),
                    observation.resource_uid.as_str(),
                    observation.resource_version.as_str(),
                    observation.authority_context.as_str(),
                    observation.configuration_identity.as_str(),
                    observation.revision,
                    observation.observed_after.as_str(),
                    observation.fresh,
                    observation.provider_id.as_str(),
                ))
            )
}

#[must_use]
pub fn gateway_observation_integrity_is_valid(
    observation: &GatewayObservation,
    provider: &dyn GatewayObservationProvider,
) -> bool {
    gateway_observation_content_integrity_is_valid(observation)
        && observation.provider_id == provider.provider_id()
        && provider.verify(&observation.observation_id, &observation.provider_proof)
}

fn digest_json(value: &impl Serialize) -> String {
    extraction_input_digest(serde_json::to_vec(value).expect("edge values must serialize"))
}
