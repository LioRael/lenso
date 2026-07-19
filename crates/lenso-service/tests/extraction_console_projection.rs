use lenso_service::{
    ExtractionConsoleArtifacts, ExtractionConsoleState, project_extraction_console,
};

#[test]
fn backend_projection_keeps_rolled_back_cutover_distinct_and_read_only() {
    let projection = project_extraction_console(ExtractionConsoleArtifacts {
        readiness: Some(
            serde_json::json!({"protocol":"lenso.extraction-readiness-report.v1","ready":true,"findings":[]}),
        ),
        plan: Some(
            serde_json::json!({"protocol":"lenso.extraction-plan.v1","planId":"plan:support","planDigest":"sha256:plan","phases":[{"phaseId":"07-provisional-cutover","kind":"provisional_cutover"}],"approvalBoundaries":[]}),
        ),
        phase_artifacts: vec![
            serde_json::json!({"protocol":"lenso.extraction-provisional-cutover.v1","cutoverId":"cutover:failed","status":"rolled_back","evidence":[{"kind":"rollback","subject":"operator:alice","digest":"sha256:rollback","detail":"candidate returned 503"}]}),
        ],
        authority: Some(
            serde_json::json!({"kind":"linked_host","ownerId":"support-host","revision":"authority-r7"}),
        ),
    });
    assert_eq!(projection.state, ExtractionConsoleState::RolledBack);
    assert_eq!(projection.plan_id.as_deref(), Some("plan:support"));
    assert!(projection.read_only);
    assert!(projection.apply_actions.is_empty());
    assert_eq!(projection.protected_workflow, "lenso service extract");
    assert!(
        projection
            .timeline
            .iter()
            .any(|phase| phase.state == "rolled_back")
    );
}

#[test]
fn committed_autonomous_mutation_projects_post_commit_rollback_blocked() {
    let projection = project_extraction_console(ExtractionConsoleArtifacts {
        readiness: None,
        plan: Some(
            serde_json::json!({"planId":"plan:support","planDigest":"sha256:plan","phases":[]}),
        ),
        phase_artifacts: vec![
            serde_json::json!({"protocol":"lenso.extraction-authority-commit.v1","commitId":"commit:support","status":"committed","candidateAuthoritative":true,"fastRollbackBlocked":true,"autonomousMutationIds":["mutation:43"]}),
        ],
        authority: Some(
            serde_json::json!({"kind":"autonomous_service","ownerId":"support-ticket-service","revision":"authority-r8"}),
        ),
    });
    assert_eq!(
        projection.state,
        ExtractionConsoleState::PostCommitRollbackBlocked
    );
    assert_eq!(
        projection.current_authority.owner_id,
        "support-ticket-service"
    );
}

#[test]
fn latest_cutover_retry_wins_and_planned_phases_link_to_the_real_plan() {
    let projection = project_extraction_console(ExtractionConsoleArtifacts {
        readiness: None,
        plan: Some(serde_json::json!({
            "protocol":"lenso.extraction-plan.v1","planId":"plan:support","planDigest":"sha256:plan",
            "phases":[{"phaseId":"08-provisional-cutover","kind":"provisional_cutover"}]
        })),
        phase_artifacts: vec![
            serde_json::json!({"protocol":"lenso.extraction-provisional-cutover.v1","cutoverId":"cutover:first","status":"rolled_back"}),
            serde_json::json!({"protocol":"lenso.extraction-provisional-cutover.v1","cutoverId":"cutover:retry","status":"verified"}),
        ],
        authority: None,
    });
    assert_eq!(projection.state, ExtractionConsoleState::Provisional);
    assert!(
        projection
            .timeline
            .iter()
            .any(|entry| { entry.state == "planned" && entry.artifact_id == "plan:support" })
    );
}
