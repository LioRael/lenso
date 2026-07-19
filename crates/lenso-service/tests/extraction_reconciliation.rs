use lenso_service::{
    ExtractionBackfillBoundary, ExtractionBackfillRecord, ExtractionBackfillRequest,
    ExtractionBusinessInvariant, ExtractionPlan, ExtractionReconciliationInputs,
    ExtractionReconciliationIssueCode, ExtractionReconciliationStatus, ExtractionRelationshipCount,
    ExtractionRun, ExtractionSourceSnapshot, apply_extraction_backfill_batch,
    reconcile_extraction_data, start_extraction_backfill,
};

fn plan() -> ExtractionPlan {
    serde_json::from_str(include_str!(
        "../../../contracts/extraction/support-ticket.plan.json"
    ))
    .unwrap()
}

fn expansion() -> ExtractionRun {
    serde_json::from_str(include_str!(
        "../../../contracts/extraction/support-ticket.expansion-run.json"
    ))
    .unwrap()
}

fn record(id: &str, status: &str) -> ExtractionBackfillRecord {
    ExtractionBackfillRecord::new(id, serde_json::json!({"id": id, "status": status}))
}

fn completed_backfill() -> lenso_service::ExtractionBackfillRun {
    let run = start_extraction_backfill(
        &plan(),
        &expansion(),
        ExtractionBackfillBoundary::TrustworthyCursor {
            cursor: "support_tickets.id".to_owned(),
            source_high_water_mark: "ticket-002".to_owned(),
        },
    )
    .unwrap();
    apply_extraction_backfill_batch(
        run,
        ExtractionBackfillRequest::new(
            "batch-001",
            None,
            vec![record("ticket-001", "open"), record("ticket-002", "closed")],
        )
        .final_batch(),
    )
    .unwrap()
}

fn source() -> ExtractionSourceSnapshot {
    ExtractionSourceSnapshot {
        source_high_water_mark: "ticket-002".to_owned(),
        records: vec![record("ticket-001", "open"), record("ticket-002", "closed")],
        relationship_counts: vec![ExtractionRelationshipCount::new("ticket-comments", 3)],
    }
}

#[test]
fn matching_data_records_checkpointed_business_evidence() {
    let result = reconcile_extraction_data(ExtractionReconciliationInputs {
        backfill: completed_backfill(),
        source: source(),
        destination_records: None,
        destination_relationship_counts: vec![ExtractionRelationshipCount::new(
            "ticket-comments",
            3,
        )],
        normalized_fields: vec![],
        business_invariants: vec![ExtractionBusinessInvariant::passed(
            "closed-ticket-has-resolution",
            "ticket-002 has a resolution event",
        )],
    });

    assert_eq!(result.status, ExtractionReconciliationStatus::Matched);
    assert!(result.issues.is_empty());
    assert!(result.linked_authority_remains_authoritative);
    assert!(!result.candidate_writes_admitted);
    assert!(result.evidence.iter().any(|e| e.kind == "record_digest"));
}

#[test]
fn each_mismatch_class_has_a_stable_blocking_code_and_next_action() {
    let mut changed_source = source();
    changed_source.records[0] = record("ticket-001", "waiting");
    changed_source.relationship_counts[0].count = 4;
    let result = reconcile_extraction_data(ExtractionReconciliationInputs {
        backfill: completed_backfill(),
        source: changed_source,
        destination_records: None,
        destination_relationship_counts: vec![ExtractionRelationshipCount::new(
            "ticket-comments",
            3,
        )],
        normalized_fields: vec![],
        business_invariants: vec![ExtractionBusinessInvariant::failed(
            "closed-ticket-has-resolution",
            "ticket-002 is missing its resolution event",
        )],
    });

    assert_eq!(result.status, ExtractionReconciliationStatus::Blocked);
    let codes = result
        .issues
        .iter()
        .map(|issue| issue.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&ExtractionReconciliationIssueCode::FieldDigestMismatch));
    assert!(codes.contains(&ExtractionReconciliationIssueCode::RelationshipCountMismatch));
    assert!(codes.contains(&ExtractionReconciliationIssueCode::BusinessInvariantMismatch));
    assert!(
        result
            .issues
            .iter()
            .all(|issue| !issue.next_actions.is_empty())
    );
}

#[test]
fn a_changed_source_high_water_mark_invalidates_prior_reconciliation() {
    let mut source = source();
    source.source_high_water_mark = "ticket-003".to_owned();
    let result = reconcile_extraction_data(ExtractionReconciliationInputs {
        backfill: completed_backfill(),
        source,
        destination_records: None,
        destination_relationship_counts: vec![ExtractionRelationshipCount::new(
            "ticket-comments",
            3,
        )],
        normalized_fields: vec![],
        business_invariants: vec![],
    });
    assert_eq!(result.status, ExtractionReconciliationStatus::Blocked);
    assert_eq!(
        result.issues[0].code,
        ExtractionReconciliationIssueCode::SourceStateChanged
    );
}
