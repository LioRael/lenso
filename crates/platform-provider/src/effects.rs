use crate::{
    ProviderConfig, ProviderHostEffectBatch, ProviderHostEventEffect,
    ProviderHostRuntimeFunctionRequest, ProviderInvocation, ProviderOutcome, ProviderOutcomeStatus,
};
use platform_core::{
    AppError, AppResult, CorrelationId, DbPool, ErrorCode, OutboxEvent, OutboxPublisher, TenantId,
};
use platform_runtime::{EnqueueFunctionRequest, FunctionTenancyMode, RuntimeClient};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

#[derive(Debug, Clone)]
pub struct ProviderHostEffectCoordinator {
    pool: Option<DbPool>,
}

impl ProviderHostEffectCoordinator {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool: Some(pool) }
    }

    #[must_use]
    pub fn rejecting() -> Self {
        Self { pool: None }
    }

    pub async fn commit(
        &self,
        config: &ProviderConfig,
        invocation: &ProviderInvocation,
        outcome: &ProviderOutcome,
    ) -> AppResult<()> {
        let effects = &outcome.host_effects;
        if effects.events.is_empty() && effects.runtime_function_requests.is_empty() {
            return Ok(());
        }
        let pool = self.pool.as_ref().ok_or_else(|| {
            AppError::new(
                ErrorCode::Internal,
                "Provider Host effect coordinator is not configured",
            )
        })?;
        if !matches!(outcome.status, ProviderOutcomeStatus::Succeeded) {
            return Err(AppError::new(
                ErrorCode::ExternalDependency,
                "Provider returned Host effects for a non-succeeded outcome",
            ));
        }
        validate_effects(config, invocation, effects)?;
        let effects_digest = digest(effects)?;
        let service_release_digest = config.service_release_digest.as_deref().ok_or_else(|| {
            AppError::new(
                ErrorCode::Internal,
                "Provider Service Release is not locked",
            )
        })?;
        let module_release_digest = config.module_release_digest.as_deref().ok_or_else(|| {
            AppError::new(ErrorCode::Internal, "Provider Module Release is not locked")
        })?;

        let mut tx = pool.begin().await.map_err(map_store_error)?;
        let inserted = sqlx::query_scalar::<_, bool>(
            r#"
            insert into platform.provider_host_effect_commits (
                invocation_id, outcome_digest, effects_digest,
                service_release_digest, module_release_digest, export_key
            )
            values ($1, $2, $3, $4, $5, $6)
            on conflict (invocation_id) do nothing
            returning true
            "#,
        )
        .bind(&invocation.invocation_id)
        .bind(&outcome.outcome_digest)
        .bind(&effects_digest)
        .bind(service_release_digest)
        .bind(module_release_digest)
        .bind(&config.export_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_store_error)?
        .unwrap_or(false);

        if !inserted {
            let existing = sqlx::query_as::<_, (String, String)>(
                r#"
                select outcome_digest, effects_digest
                from platform.provider_host_effect_commits
                where invocation_id = $1
                "#,
            )
            .bind(&invocation.invocation_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_store_error)?;
            if existing != (outcome.outcome_digest.clone(), effects_digest) {
                return Err(AppError::new(
                    ErrorCode::Conflict,
                    "Provider invocation attempted to rebind committed Host effects",
                ));
            }
            tx.commit().await.map_err(map_store_error)?;
            return Ok(());
        }

        let outbox = OutboxPublisher;
        for event in &effects.events {
            outbox.publish_in_tx(&mut tx, &outbox_event(event)).await?;
        }
        let runtime = RuntimeClient::new(pool.clone());
        for request in &effects.runtime_function_requests {
            runtime
                .enqueue_function_with_id_in_tx(
                    &mut tx,
                    &request.request_id,
                    runtime_request(request),
                )
                .await?;
        }
        tx.commit().await.map_err(map_store_error)
    }

    pub async fn mark_acknowledged(
        &self,
        invocation_id: &str,
        outcome_digest: &str,
    ) -> AppResult<()> {
        let Some(pool) = self.pool.as_ref() else {
            return Ok(());
        };
        sqlx::query(
            r#"
            update platform.provider_host_effect_commits
            set acknowledged_at = coalesce(acknowledged_at, now())
            where invocation_id = $1 and outcome_digest = $2
            "#,
        )
        .bind(invocation_id)
        .bind(outcome_digest)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(map_store_error)
    }
}

fn validate_effects(
    config: &ProviderConfig,
    invocation: &ProviderInvocation,
    effects: &ProviderHostEffectBatch,
) -> AppResult<()> {
    for event in &effects.events {
        if event.event_id.trim().is_empty()
            || event.event_name.trim().is_empty()
            || event.aggregate_type.trim().is_empty()
            || event.aggregate_id.trim().is_empty()
            || event.source_module != config.name
            || event.correlation_id != invocation.correlation_id
        {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Provider Host Event effect is not bound to the locked Module invocation",
            ));
        }
    }
    for request in &effects.runtime_function_requests {
        if request.request_id.trim().is_empty()
            || request.correlation_id != invocation.correlation_id
            || !config
                .allowed_host_function_names
                .contains(&request.function_name)
            || request
                .max_attempts
                .is_some_and(|value| !(1..=100).contains(&value))
        {
            return Err(AppError::new(
                ErrorCode::Validation,
                "Provider Runtime Function effect is not bound to the locked Module invocation",
            ));
        }
    }
    Ok(())
}

fn outbox_event(effect: &ProviderHostEventEffect) -> OutboxEvent {
    OutboxEvent {
        id: effect.event_id.clone(),
        event_name: effect.event_name.clone(),
        event_version: effect.event_version,
        source_module: effect.source_module.clone(),
        aggregate_type: effect.aggregate_type.clone(),
        aggregate_id: effect.aggregate_id.clone(),
        correlation_id: effect.correlation_id.clone(),
        causation_id: effect.causation_id.clone(),
        occurred_at: effect.occurred_at,
        payload: effect.payload.clone(),
        headers: effect.headers.clone(),
    }
}

fn runtime_request(effect: &ProviderHostRuntimeFunctionRequest) -> EnqueueFunctionRequest {
    EnqueueFunctionRequest {
        function_name: effect.function_name.clone(),
        input_json: effect.input.clone(),
        correlation_id: CorrelationId::new(effect.correlation_id.clone()),
        actor: effect.actor.clone(),
        tenant_id: effect.tenant_id.clone().map(TenantId),
        tenancy_mode: FunctionTenancyMode::Optional,
        trace: effect.trace.clone(),
        causation_id: effect.causation_id.clone(),
        max_attempts: effect.max_attempts,
    }
}

fn digest(value: &ProviderHostEffectBatch) -> AppResult<String> {
    let encoded = serde_json::to_vec(value).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("Provider Host effects could not be encoded: {error}"),
        )
    })?;
    let mut digest = String::from("sha256:");
    for byte in Sha256::digest(encoded) {
        write!(digest, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(digest)
}

fn map_store_error(error: sqlx::Error) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("Provider Host effect Store operation failed: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PROVIDER_PROTOCOL, ProviderHostEventEffect, ProviderHostRuntimeFunctionRequest,
        ProviderInvocationMode, ProviderOperationKind,
    };
    use chrono::Utc;
    use platform_core::{ActorContext, TraceContext, apply_migrations};
    use platform_testing::TestDatabase;
    use serde_json::json;

    #[tokio::test]
    async fn commits_host_effects_atomically_and_replays_without_duplicates() {
        let Some(db) = TestDatabase::create().await else {
            return;
        };
        apply_migrations(&db.pool, platform_core::PLATFORM_MIGRATIONS)
            .await
            .unwrap();
        apply_migrations(&db.pool, platform_runtime::RUNTIME_MIGRATIONS)
            .await
            .unwrap();

        let config = ProviderConfig::new("lenso/support", "http://provider.test")
            .with_export_key("support")
            .with_locked_contract(
                digest_value('1'),
                digest_value('2'),
                digest_value('3'),
                vec![],
            )
            .with_allowed_host_functions(["support.follow_up.v1".to_owned()]);
        let invocation = invocation("invocation-1", "correlation-1");
        let outcome = outcome("invocation-1", "correlation-1");
        let coordinator = ProviderHostEffectCoordinator::new(db.pool.clone());

        coordinator
            .commit(&config, &invocation, &outcome)
            .await
            .unwrap();
        coordinator
            .commit(&config, &invocation, &outcome)
            .await
            .unwrap();

        assert_eq!(
            count(&db.pool, "platform.provider_host_effect_commits").await,
            1
        );
        assert_eq!(count(&db.pool, "platform.outbox").await, 1);
        assert_eq!(count(&db.pool, "runtime.function_runs").await, 1);

        let mut rebound = outcome;
        rebound.outcome_digest = digest_value('9');
        let error = coordinator
            .commit(&config, &invocation, &rebound)
            .await
            .expect_err("committed invocation identity cannot be rebound");
        assert_eq!(error.code, ErrorCode::Conflict);

        db.cleanup().await;
    }

    fn invocation(id: &str, correlation_id: &str) -> ProviderInvocation {
        ProviderInvocation {
            protocol: PROVIDER_PROTOCOL.to_owned(),
            invocation_id: id.to_owned(),
            request_id: id.to_owned(),
            attempt: 1,
            deadline: Utc::now().to_rfc3339(),
            service_release_digest: digest_value('1'),
            export_key: "support".to_owned(),
            module_release_digest: digest_value('2'),
            manifest_digest: digest_value('3'),
            operation_kind: ProviderOperationKind::AdminAction,
            operation_name: "support.act".to_owned(),
            operation_version: "1".to_owned(),
            mode: ProviderInvocationMode::Durable,
            input_contract_digest: digest_value('4'),
            output_contract_digest: digest_value('4'),
            tenant_id: None,
            actor: ActorContext::System,
            delegation: None,
            locale: None,
            context: Default::default(),
            correlation_id: correlation_id.to_owned(),
            causation_id: None,
            trace: TraceContext::default(),
            content_type: "application/json".to_owned(),
            payload: json!({}),
        }
    }

    fn outcome(id: &str, correlation_id: &str) -> ProviderOutcome {
        ProviderOutcome {
            protocol: PROVIDER_PROTOCOL.to_owned(),
            invocation_id: id.to_owned(),
            status: ProviderOutcomeStatus::Succeeded,
            result: Some(json!({ "ok": true })),
            error: None,
            effect_evidence: vec![],
            host_effects: ProviderHostEffectBatch {
                events: vec![ProviderHostEventEffect {
                    event_id: "event-1".to_owned(),
                    event_name: "support.updated.v1".to_owned(),
                    event_version: 1,
                    source_module: "lenso/support".to_owned(),
                    aggregate_type: "ticket".to_owned(),
                    aggregate_id: "ticket-1".to_owned(),
                    correlation_id: correlation_id.to_owned(),
                    causation_id: Some(id.to_owned()),
                    occurred_at: Utc::now(),
                    payload: json!({ "ticketId": "ticket-1" }),
                    headers: json!({}),
                }],
                runtime_function_requests: vec![ProviderHostRuntimeFunctionRequest {
                    request_id: "fnrun-provider-effect-1".to_owned(),
                    function_name: "support.follow_up.v1".to_owned(),
                    input: json!({ "ticketId": "ticket-1" }),
                    correlation_id: correlation_id.to_owned(),
                    actor: ActorContext::System,
                    tenant_id: None,
                    trace: TraceContext::default(),
                    causation_id: Some(id.to_owned()),
                    max_attempts: Some(3),
                }],
            },
            outcome_digest: digest_value('8'),
        }
    }

    fn digest_value(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    async fn count(pool: &DbPool, table: &str) -> i64 {
        let query = format!("select count(*) from {table}");
        sqlx::query_scalar(sqlx::AssertSqlSafe(query))
            .fetch_one(pool)
            .await
            .unwrap()
    }
}
