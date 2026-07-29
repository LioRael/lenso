use chrono::Utc;
use lenso_service::system_plane::{
    RUNTIME_OBSERVABILITY_PROTOCOL, RuntimeObservabilityMessage, RuntimeObservabilitySnapshot,
    RuntimeObservabilityStatus, RuntimeObservationContinuity, RuntimeObservationEvidenceGap,
    RuntimeObservationFeed, RuntimeObservationGapReason, RuntimeQueueKind, RuntimeQueueSummary,
    runtime_observability_schema, runtime_observability_schema_digest,
};

fn snapshot() -> RuntimeObservabilitySnapshot {
    RuntimeObservabilitySnapshot {
        protocol: RUNTIME_OBSERVABILITY_PROTOCOL.to_owned(),
        service_id: "support".to_owned(),
        service_revision: "release:sha256:0123456789abcdef".to_owned(),
        snapshot_revision: format!("sha256:{}", "a".repeat(64)),
        schema_digest: format!("sha256:{}", "b".repeat(64)),
        next_cursor: "opaque-snapshot-cursor".to_owned(),
        observed_at: Utc::now(),
        status: RuntimeObservabilityStatus::Healthy,
        queues: vec![RuntimeQueueSummary {
            queue: RuntimeQueueKind::Outbox,
            pending: 0,
            active: 0,
            completed: 12,
            failed: 0,
            dead: 0,
            oldest_pending_age_seconds: None,
            oldest_failed_age_seconds: None,
        }],
    }
}

#[test]
fn runtime_observability_snapshot_is_a_strict_ui_neutral_wire_contract() {
    let mut value = serde_json::to_value(snapshot()).unwrap();

    assert_eq!(value["protocol"], RUNTIME_OBSERVABILITY_PROTOCOL);
    assert_eq!(value["queues"][0]["queue"], "outbox");
    assert!(value["queues"][0].get("pageRoute").is_none());
    value["workspace"] = serde_json::json!("runtime");
    assert!(serde_json::from_value::<RuntimeObservabilitySnapshot>(value).is_err());
}

#[test]
fn schema_and_digest_validate_the_exact_snapshot_shape() {
    let schema = runtime_observability_schema();
    let validator = jsonschema::validator_for(&schema).unwrap();

    assert!(validator.is_valid(
        &serde_json::to_value(RuntimeObservabilityMessage::Snapshot(snapshot())).unwrap()
    ));
    assert!(
        runtime_observability_schema_digest()
            .strip_prefix("sha256:")
            .is_some_and(|digest| digest.len() == 64)
    );
    assert_eq!(
        schema["$defs"]["RuntimeObservabilitySnapshot"]["properties"]["protocol"]["const"],
        RUNTIME_OBSERVABILITY_PROTOCOL
    );
}

#[test]
fn recovery_feed_makes_loss_of_continuity_explicit() {
    let feed = RuntimeObservationFeed {
        protocol: RUNTIME_OBSERVABILITY_PROTOCOL.to_owned(),
        service_id: "support".to_owned(),
        service_revision: "release:sha256:0123456789abcdef".to_owned(),
        schema_digest: format!("sha256:{}", "b".repeat(64)),
        collected_at: Utc::now(),
        continuity: RuntimeObservationContinuity::ResetRequired,
        evidence_gap: Some(RuntimeObservationEvidenceGap {
            reason: RuntimeObservationGapReason::RetentionLost,
            message: "Required changes were pruned.".to_owned(),
            required_action: "fetch_fresh_runtime_observability_snapshot".to_owned(),
        }),
        changes: Vec::new(),
        next_cursor: String::new(),
        has_more: false,
    };
    let value = serde_json::to_value(RuntimeObservabilityMessage::Feed(feed)).unwrap();

    assert!(
        jsonschema::validator_for(&runtime_observability_schema())
            .unwrap()
            .is_valid(&value)
    );
    assert_eq!(value["document"]["continuity"], "reset_required");
    assert_eq!(value["document"]["evidenceGap"]["reason"], "retention_lost");
}
