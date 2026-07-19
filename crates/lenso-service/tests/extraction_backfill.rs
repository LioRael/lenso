use lenso_service::{
    ExtractionBackfillBoundary, ExtractionBackfillRecord, ExtractionBackfillRequest,
    ExtractionBackfillStatus, ExtractionPlan, ExtractionRun, apply_extraction_backfill_batch,
    start_extraction_backfill,
};

fn plan() -> ExtractionPlan {
    serde_json::from_str(include_str!(
        "../../../contracts/extraction/support-ticket.plan.json"
    ))
    .expect("generated plan fixture")
}

fn expansion() -> ExtractionRun {
    serde_json::from_str(include_str!(
        "../../../contracts/extraction/support-ticket.expansion-run.json"
    ))
    .expect("generated expansion fixture")
}

fn record(id: &str, title: &str) -> ExtractionBackfillRecord {
    ExtractionBackfillRecord::new(id, serde_json::json!({"id": id, "title": title}))
}

#[test]
fn interrupted_backfill_resumes_from_a_durable_checkpoint_without_duplicates() {
    let plan = plan();
    let mut run = start_extraction_backfill(
        &plan,
        &expansion(),
        ExtractionBackfillBoundary::TrustworthyCursor {
            cursor: "support_tickets.updated_at,id".to_owned(),
            source_high_water_mark: "2026-07-19T00:00:00Z/ticket-003".to_owned(),
        },
    )
    .expect("backfill can start after destination expansion");

    run = apply_extraction_backfill_batch(
        run,
        ExtractionBackfillRequest::new(
            "batch-001",
            None,
            vec![
                record("ticket-001", "Cannot sign in"),
                record("ticket-002", "Billing"),
            ],
        ),
    )
    .expect("first durable batch");
    let checkpoint = run.progress.destination_checkpoint.clone().unwrap();

    let resumed = apply_extraction_backfill_batch(
        run.clone(),
        ExtractionBackfillRequest::new(
            "batch-001",
            None,
            vec![
                record("ticket-001", "Cannot sign in"),
                record("ticket-002", "Billing"),
            ],
        ),
    )
    .expect("repeating the committed batch is idempotent");
    assert_eq!(resumed, run);

    run = apply_extraction_backfill_batch(
        resumed,
        ExtractionBackfillRequest::new(
            "batch-002",
            Some(checkpoint),
            vec![record("ticket-003", "Export")],
        )
        .final_batch(),
    )
    .expect("resume from durable destination checkpoint");

    assert_eq!(run.status, ExtractionBackfillStatus::Succeeded);
    assert_eq!(run.progress.copied_count, 3);
    assert_eq!(run.progress.remaining_lag, 0);
    assert_eq!(run.destination_records.len(), 3);
    assert!(run.linked_authority_remains_authoritative);
    assert!(!run.candidate_authoritative);
}

#[test]
fn online_backfill_requires_a_trustworthy_cursor_but_write_pause_can_bound_copying() {
    let error =
        start_extraction_backfill(&plan(), &expansion(), ExtractionBackfillBoundary::Missing)
            .expect_err("online preparation must fail closed without a cursor");
    assert_eq!(error.code.as_str(), "backfill_cursor_missing");

    let run = start_extraction_backfill(
        &plan(),
        &expansion(),
        ExtractionBackfillBoundary::BoundedWritePause {
            source_high_water_mark: "support-write-pause/r8".to_owned(),
        },
    )
    .expect("the protected write-pause phase supplies a stable boundary");
    assert_eq!(run.status, ExtractionBackfillStatus::Planned);
    assert!(run.linked_authority_remains_authoritative);
}

#[test]
fn batches_are_plan_scoped_and_deterministically_ordered() {
    let run = start_extraction_backfill(
        &plan(),
        &expansion(),
        ExtractionBackfillBoundary::TrustworthyCursor {
            cursor: "support_tickets.id".to_owned(),
            source_high_water_mark: "ticket-002".to_owned(),
        },
    )
    .unwrap();
    let error = apply_extraction_backfill_batch(
        run,
        ExtractionBackfillRequest::new(
            "batch-001",
            None,
            vec![
                record("ticket-002", "second"),
                record("ticket-001", "first"),
            ],
        ),
    )
    .expect_err("unstable ordering must not be accepted");
    assert_eq!(error.code.as_str(), "backfill_batch_unordered");
}
