use crate::{
    PROVIDER_PROTOCOL, ProviderConfig, ProviderErrorBody, ProviderHostEffectCoordinator,
    ProviderInvocation, ProviderInvocationAcknowledgement, ProviderInvocationMode,
    ProviderOperationKind, ProviderOutcome, ProviderOutcomeStatus, ProviderTransport,
};
use platform_core::{ActorContext, AppError, AppResult, ErrorCode, TraceContext};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

pub(crate) struct InvocationContext {
    pub invocation_id: String,
    pub request_id: String,
    pub attempt: u32,
    pub actor: ActorContext,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub trace: TraceContext,
}

pub(crate) fn build(
    config: &ProviderConfig,
    kind: ProviderOperationKind,
    operation_name: impl Into<String>,
    operation_version: impl Into<String>,
    mode: ProviderInvocationMode,
    context: InvocationContext,
    payload: Value,
) -> AppResult<ProviderInvocation> {
    let service_release_digest = locked(&config.service_release_digest, "Service Release")?;
    let module_release_digest = locked(&config.module_release_digest, "Module Release")?;
    let manifest_digest = locked(&config.manifest_digest, "Manifest")?;
    let contract_digest = match config.contract_digests.as_slice() {
        [digest] => digest.clone(),
        [] => {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Provider Export has no locked operation contract digest",
            ));
        }
        _ => {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Provider Export has multiple unkeyed contract digests; an exact operation contract cannot be selected",
            ));
        }
    };
    if config.export_key.is_empty() {
        return Err(AppError::new(
            ErrorCode::Validation,
            "Provider Export key is not locked",
        ));
    }
    let deadline = chrono::Utc::now()
        + chrono::Duration::milliseconds(i64::try_from(config.timeout_ms).unwrap_or(i64::MAX));
    Ok(ProviderInvocation {
        protocol: PROVIDER_PROTOCOL.to_owned(),
        invocation_id: context.invocation_id,
        request_id: context.request_id,
        attempt: context.attempt,
        deadline: deadline.to_rfc3339(),
        service_release_digest,
        export_key: config.export_key.clone(),
        module_release_digest,
        manifest_digest,
        operation_kind: kind,
        operation_name: operation_name.into(),
        operation_version: operation_version.into(),
        mode,
        input_contract_digest: contract_digest.clone(),
        output_contract_digest: contract_digest,
        tenant_id: None,
        actor: context.actor,
        delegation: None,
        locale: None,
        context: std::collections::BTreeMap::new(),
        correlation_id: context.correlation_id,
        causation_id: context.causation_id,
        trace: context.trace,
        content_type: "application/json".to_owned(),
        payload,
    })
}

pub(crate) async fn send(
    client: &reqwest::Client,
    config: &ProviderConfig,
    effects: &ProviderHostEffectCoordinator,
    binding: &str,
    invocation: &ProviderInvocation,
) -> AppResult<ProviderOutcome> {
    if config.transport == ProviderTransport::Grpc {
        let outcome = match crate::grpc::invoke(config, binding, invocation).await {
            Ok(outcome) => outcome,
            Err(_) => crate::grpc::get_invocation(config, &invocation.invocation_id).await?,
        };
        return finalize(client, config, effects, invocation, outcome).await;
    }
    let url = format!(
        "{}/exports/{}/{}",
        config.base_url, config.export_key, binding
    );
    let mut request = client.post(url).json(invocation);
    if let Some(token) = &config.auth_token {
        request = request.bearer_auth(token);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(_) => {
            let outcome = get_http_invocation(client, config, &invocation.invocation_id).await?;
            return finalize(client, config, effects, invocation, outcome).await;
        }
    };
    let status = response.status();
    let bytes = response.bytes().await.map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("Provider outcome could not be read: {error}"),
        )
    })?;
    if !status.is_success() {
        let error = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|value| value.get("error").cloned())
            .and_then(|value| serde_json::from_value::<ProviderErrorBody>(value).ok());
        return Err(AppError::new(
            ErrorCode::ExternalDependency,
            error.map_or_else(
                || format!("Provider invocation returned {status}"),
                |error| format!("{}: {}", error.code, error.message),
            ),
        ));
    }
    let outcome: ProviderOutcome = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("Provider outcome was invalid: {error}"),
        )
    })?;
    finalize(client, config, effects, invocation, outcome).await
}

async fn get_http_invocation(
    client: &reqwest::Client,
    config: &ProviderConfig,
    invocation_id: &str,
) -> AppResult<ProviderOutcome> {
    let url = format!("{}/invocations/{invocation_id}", config.base_url);
    let mut request = client.get(url);
    if let Some(token) = &config.auth_token {
        request = request.bearer_auth(token);
    }
    request
        .send()
        .await
        .map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("Provider invocation recovery failed: {error}"),
            )
            .retryable()
        })?
        .error_for_status()
        .map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("Provider invocation recovery returned an error: {error}"),
            )
            .retryable()
        })?
        .json()
        .await
        .map_err(|error| {
            AppError::new(
                ErrorCode::ExternalDependency,
                format!("Provider recovered outcome was invalid: {error}"),
            )
        })
}

async fn finalize(
    client: &reqwest::Client,
    config: &ProviderConfig,
    effects: &ProviderHostEffectCoordinator,
    invocation: &ProviderInvocation,
    outcome: ProviderOutcome,
) -> AppResult<ProviderOutcome> {
    let outcome = validate_outcome(invocation, outcome)?;
    let has_host_effects = !outcome.host_effects.events.is_empty()
        || !outcome.host_effects.runtime_function_requests.is_empty();
    effects.commit(config, invocation, &outcome).await?;
    let acknowledgement = ProviderInvocationAcknowledgement {
        invocation_id: outcome.invocation_id.clone(),
        outcome_digest: outcome.outcome_digest.clone(),
    };
    if config.transport == ProviderTransport::Grpc {
        crate::grpc::acknowledge_invocation(config, &acknowledgement).await?;
    } else {
        let url = format!(
            "{}/invocations/{}:ack",
            config.base_url, acknowledgement.invocation_id
        );
        let mut request = client.post(url).json(&acknowledgement);
        if let Some(token) = &config.auth_token {
            request = request.bearer_auth(token);
        }
        request
            .send()
            .await
            .map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("Provider invocation acknowledgement failed: {error}"),
                )
                .retryable()
            })?
            .error_for_status()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::ExternalDependency,
                    format!("Provider invocation acknowledgement was rejected: {error}"),
                )
            })?;
    }
    if has_host_effects {
        effects
            .mark_acknowledged(
                &acknowledgement.invocation_id,
                &acknowledgement.outcome_digest,
            )
            .await?;
    }
    Ok(outcome)
}

pub(crate) fn result(
    invocation: &ProviderInvocation,
    outcome: ProviderOutcome,
) -> AppResult<Value> {
    let outcome = validate_outcome(invocation, outcome)?;
    match outcome.status {
        ProviderOutcomeStatus::Succeeded => Ok(outcome.result.unwrap_or(Value::Null)),
        ProviderOutcomeStatus::Pending => Err(AppError::new(
            ErrorCode::ExternalDependency,
            "Provider invocation remains pending",
        )
        .retryable()),
        ProviderOutcomeStatus::Rejected => Err(AppError::new(
            ErrorCode::Validation,
            outcome.error.map_or_else(
                || "Provider invocation was rejected".to_owned(),
                |error| format!("{}: {}", error.code, error.message),
            ),
        )),
        ProviderOutcomeStatus::Failed => Err(AppError::new(
            ErrorCode::ExternalDependency,
            outcome.error.map_or_else(
                || "Provider invocation failed".to_owned(),
                |error| format!("{}: {}", error.code, error.message),
            ),
        )),
    }
}

fn validate_outcome(
    invocation: &ProviderInvocation,
    outcome: ProviderOutcome,
) -> AppResult<ProviderOutcome> {
    if outcome.protocol != PROVIDER_PROTOCOL || outcome.invocation_id != invocation.invocation_id {
        return Err(AppError::new(
            ErrorCode::ExternalDependency,
            "Provider outcome identity did not match the invocation",
        ));
    }
    let mut digest_input = outcome.clone();
    digest_input.outcome_digest.clear();
    let encoded = serde_json::to_vec(&digest_input).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("Provider outcome digest input could not be encoded: {error}"),
        )
    })?;
    let mut expected = String::from("sha256:");
    for byte in Sha256::digest(encoded) {
        write!(expected, "{byte:02x}").expect("writing to a String cannot fail");
    }
    if outcome.outcome_digest != expected {
        return Err(AppError::new(
            ErrorCode::ExternalDependency,
            "Provider outcome digest did not match its content",
        ));
    }
    Ok(outcome)
}

fn locked(value: &Option<String>, label: &str) -> AppResult<String> {
    value.clone().ok_or_else(|| {
        AppError::new(
            ErrorCode::Validation,
            format!("Provider {label} digest is not locked"),
        )
    })
}
