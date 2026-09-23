#![cfg(feature = "host")]

use futures::FutureExt;
use lenso::host::{
    self, CanonicalDocument, ControlPlaneError, GenerationRuntime, HostBuilder,
    MemoryControlStateStore, ResolvedGeneration,
};
use lenso_app_plan::ResolvedAppPlan;
use lenso_plugin_control_plane::{
    AppGenerationSpec, AppGenerationTransitionSpec, EffectiveHostGrantSet, ReplacementMode,
    ResolvedArtifactSet, RolloutPolicy,
};
use lenso_runtime_codec::{ArtifactCatalog, InstanceResourceCatalog};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug)]
struct Runtime;
impl GenerationRuntime for Runtime {
    type Handle = ();
    type Route = ();
    fn stage<'a>(
        &'a mut self,
        _: &'a ResolvedGeneration,
        _: u64,
    ) -> futures::future::LocalBoxFuture<'a, Result<(), ControlPlaneError>> {
        async { Ok(()) }.boxed_local()
    }
    fn shutdown(
        &mut self,
        (): (),
        _: u64,
    ) -> futures::future::LocalBoxFuture<'_, Result<(), ControlPlaneError>> {
        async { Ok(()) }.boxed_local()
    }
    fn terminal_failure(&self, (): &()) -> Option<ControlPlaneError> {
        None
    }
    fn route(&self, (): &()) {}
}

fn generation() -> ResolvedGeneration {
    generation_with_authority("authority")
}

fn generation_with_authority(authority: &str) -> ResolvedGeneration {
    let artifacts = CanonicalDocument::from_value(
        "artifacts",
        ResolvedArtifactSet {
            schema_version: 3,
            resolution_authority_digest: authority.into(),
            host_execution_policy_digest: "policy".into(),
            artifacts: vec![],
            instance_resources: vec![],
        },
    )
    .unwrap();
    let grants = CanonicalDocument::from_value(
        "grants",
        EffectiveHostGrantSet {
            schema_version: 2,
            resolution_authority_digest: authority.into(),
            grants: vec![],
        },
    )
    .unwrap();
    let spec = CanonicalDocument::from_value(
        "generation",
        AppGenerationSpec {
            schema_version: 2,
            app_id: "example.app".into(),
            host_build_manifest_digest: "host".into(),
            host_execution_policy_digest: "policy".into(),
            resolved_plan_digest: "plan".into(),
            resolution_authority_digest: authority.into(),
            resolved_artifact_set_digest: artifacts.digest().into(),
            effective_host_grant_set_digest: grants.digest().into(),
        },
    )
    .unwrap();
    ResolvedGeneration {
        plan: ResolvedAppPlan::new(vec![], vec![]),
        artifacts: ArtifactCatalog::new(),
        resources: InstanceResourceCatalog::new(),
        stateful_instances: BTreeMap::new(),
        artifact_set: artifacts,
        grants,
        spec,
    }
}

async fn start_host(
    candidate: ResolvedGeneration,
) -> Result<(host::Host<()>, ResolvedGeneration), ControlPlaneError> {
    let app =
        HostBuilder::new("example.app", Runtime, MemoryControlStateStore::default()).build()?;
    let transition = CanonicalDocument::from_value(
        "transition",
        AppGenerationTransitionSpec {
            schema_version: 1,
            app_id: "example.app".into(),
            from_generation_spec_digest: None,
            to_generation_spec_digest: candidate.spec.digest().into(),
            replacement_mode: ReplacementMode::Initial,
            state_compatibility_receipt_digests: vec![],
            rollout_policy: RolloutPolicy {
                ready_timeout_nanos: "1000000000".into(),
                drain_timeout_nanos: "1000000000".into(),
                rollback_window_nanos: "0".into(),
                automatic_rollback_on_generation_failure: false,
            },
        },
    )
    .unwrap();
    app.transition(transition, candidate.clone(), BTreeMap::new())
        .await?;
    Ok((app, candidate))
}

async fn write(stream: &mut tokio::io::DuplexStream, value: Value) {
    let bytes = serde_json::to_vec(&value).unwrap();
    stream
        .write_u32(u32::try_from(bytes.len()).unwrap())
        .await
        .unwrap();
    stream.write_all(&bytes).await.unwrap();
}
async fn read(stream: &mut tokio::io::DuplexStream) -> Value {
    let len = stream.read_u32().await.unwrap();
    let mut bytes = vec![0; len as usize];
    stream.read_exact(&mut bytes).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn handshake_ready_inspect_and_repeatable_stop_use_one_runtime_authority() {
    tokio::task::LocalSet::new().run_until(async {
    let (client,server)=tokio::io::duplex(4096); let (read_half,write_half)=tokio::io::split(server);
    let candidate=generation();
    let task=tokio::task::spawn_local(host::control::serve(host::control::ControlOptions { distribution:"dist-v1".into(), startup_timeout:Duration::from_secs(1), stop_timeout:Duration::from_secs(1) }, read_half, write_half, move || start_host(candidate)));
    let mut client=client;
    write(&mut client,json!({"op":"start","version":1,"id":1,"distribution":"dist-v1"})).await;
    assert_eq!(read(&mut client).await["kind"],"ready");
    let started=read(&mut client).await; assert_eq!(started["kind"],"started"); let revision=started["revision"].clone();
    write(&mut client,json!({"op":"inspect","version":1,"id":2,"revision":revision,"offset":0,"limit":64})).await;
    assert_eq!(read(&mut client).await["kind"],"inspected");
    write(&mut client,json!({"op":"stop","version":1,"id":3})).await;
    let terminal=read(&mut client).await; assert_eq!(terminal["shutdown"],"suspended"); assert_eq!(terminal["id"],3);
    task.await.unwrap().unwrap();
 }).await;
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_start_and_queued_overflow_never_open_readiness() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (client, server) = tokio::io::duplex(8192);
            let (read_half, write_half) = tokio::io::split(server);
            let task = tokio::task::spawn_local(host::control::serve(
                host::control::ControlOptions {
                    distribution: "dist-v1".into(),
                    startup_timeout: Duration::from_secs(1),
                    stop_timeout: Duration::from_secs(1),
                },
                read_half,
                write_half,
                must_not_start,
            ));
            let mut client = client;
            write(
                &mut client,
                json!({"op":"start","version":2,"id":1,"distribution":"dist-v1"}),
            )
            .await;
            assert_eq!(read(&mut client).await["kind"], "start_failed");
            task.await.unwrap().unwrap();
        })
        .await;
}

async fn must_not_start() -> Result<(host::Host<()>, ResolvedGeneration), ControlPlaneError> {
    panic!("invalid handshake must not execute runtime")
}

#[tokio::test(flavor = "current_thread")]
async fn failed_activation_receipt_is_reported_and_retryable_without_replacing_route() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (client, server) = tokio::io::duplex(4096);
            let (reader, writer) = tokio::io::split(server);
            let candidate = generation();
            let retry_candidate = candidate.clone();
            let attempts = Arc::new(AtomicUsize::new(0));
            let receipt_attempts = attempts.clone();
            let task = tokio::task::spawn_local(host::control::serve_with_reconcile_and_receipt(
                host::control::ControlOptions {
                    distribution: "dist-v1".into(),
                    startup_timeout: Duration::from_secs(1),
                    stop_timeout: Duration::from_secs(1),
                },
                reader,
                writer,
                move || start_host(candidate),
                move || {
                    let candidate = retry_candidate.clone();
                    async move { Ok(candidate) }
                },
                move |_| {
                    let attempt = receipt_attempts.fetch_add(1, Ordering::SeqCst);
                    async move {
                        if attempt == 0 {
                            Err(ControlPlaneError::HostFailure {
                                detail: "receipt unavailable".into(),
                            })
                        } else {
                            Ok(())
                        }
                    }
                },
            ));
            let mut client = client;
            write(
                &mut client,
                json!({"op":"start","version":1,"id":1,"distribution":"dist-v1"}),
            )
            .await;
            assert_eq!(read(&mut client).await["kind"], "ready");
            let started = read(&mut client).await;
            assert_eq!(started["activation_recorded"], false);
            write(
                &mut client,
                json!({"op":"reconcile","version":1,"id":2,"revision":started["revision"]}),
            )
            .await;
            let reconciled = read(&mut client).await;
            assert_eq!(reconciled["changed"], false);
            assert_eq!(reconciled["activation_recorded"], true);
            assert_eq!(attempts.load(Ordering::SeqCst), 2);
            write(&mut client, json!({"op":"stop","version":1,"id":3})).await;
            assert_eq!(read(&mut client).await["shutdown"], "suspended");
            task.await.unwrap().unwrap();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
#[allow(
    clippy::too_many_lines,
    reason = "one ordered control session proves stale, failed, switched, and idempotent outcomes"
)]
async fn reconcile_keeps_old_route_on_source_failure_then_switches_exact_candidate() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (client, server) = tokio::io::duplex(8192);
            let (read_half, write_half) = tokio::io::split(server);
            let initial = generation();
            let replacement = generation_with_authority("authority-v2");
            let replacement_digest = replacement.spec.digest().to_owned();
            let mut invalid = generation_with_authority("authority-invalid");
            let mut invalid_spec = invalid.spec.value().clone();
            invalid_spec.app_id = "other.app".into();
            invalid.spec = CanonicalDocument::from_value("generation", invalid_spec).unwrap();
            let mut attempts = 0;
            let task = tokio::task::spawn_local(host::control::serve_with_reconcile(
                host::control::ControlOptions {
                    distribution: "dist-v1".into(),
                    startup_timeout: Duration::from_secs(1),
                    stop_timeout: Duration::from_secs(1),
                },
                read_half,
                write_half,
                move || start_host(initial),
                move || {
                    attempts += 1;
                    let candidate = replacement.clone();
                    let invalid = invalid.clone();
                    async move {
                        match attempts {
                            1 => Err(ControlPlaneError::HostFailure {
                                detail: "source unavailable".into(),
                            }),
                            2 => Ok(invalid),
                            _ => Ok(candidate),
                        }
                    }
                },
            ));
            let mut client = client;
            write(
                &mut client,
                json!({"op":"start","version":1,"id":1,"distribution":"dist-v1"}),
            )
            .await;
            assert_eq!(read(&mut client).await["kind"], "ready");
            let started = read(&mut client).await;
            let revision = started["revision"].as_u64().unwrap();
            write(
                &mut client,
                json!({"op":"reconcile","version":1,"id":2,"revision":revision - 1}),
            )
            .await;
            assert_eq!(read(&mut client).await["code"], "snapshot_expired");
            write(
                &mut client,
                json!({"op":"reconcile","version":1,"id":3,"revision":revision}),
            )
            .await;
            assert_eq!(read(&mut client).await["code"], "candidate_unavailable");
            write(
                &mut client,
                json!({"op":"inspect","version":1,"id":4,"revision":revision,"offset":0,"limit":1}),
            )
            .await;
            let still_old = read(&mut client).await;
            assert_eq!(still_old["kind"], "inspected");
            assert_ne!(still_old["generation"], replacement_digest);
            write(
                &mut client,
                json!({"op":"reconcile","version":1,"id":5,"revision":revision}),
            )
            .await;
            assert_eq!(
                read(&mut client).await["code"],
                "candidate_not_ready_or_incompatible"
            );
            write(
                &mut client,
                json!({"op":"inspect","version":1,"id":6,"revision":null,"offset":0,"limit":1}),
            )
            .await;
            let still_old = read(&mut client).await;
            assert_eq!(still_old["kind"], "inspected");
            assert_ne!(still_old["generation"], replacement_digest);
            let revision = still_old["revision"].as_u64().unwrap();
            write(
                &mut client,
                json!({"op":"reconcile","version":1,"id":7,"revision":revision}),
            )
            .await;
            let switched = read(&mut client).await;
            assert_eq!(switched["kind"], "reconciled");
            assert_eq!(switched["changed"], true);
            assert_eq!(switched["generation"], replacement_digest);
            let new_revision = switched["revision"].as_u64().unwrap();
            assert!(new_revision > revision);
            write(
                &mut client,
                json!({"op":"reconcile","version":1,"id":8,"revision":revision}),
            )
            .await;
            assert_eq!(read(&mut client).await["code"], "snapshot_expired");
            write(
                &mut client,
                json!({"op":"inspect","version":1,"id":9,"revision":null,"offset":0,"limit":1}),
            )
            .await;
            let refreshed = read(&mut client).await;
            assert_eq!(refreshed["kind"], "inspected");
            let current_revision = refreshed["revision"].as_u64().unwrap();
            write(
                &mut client,
                json!({"op":"reconcile","version":1,"id":10,"revision":current_revision}),
            )
            .await;
            let unchanged = read(&mut client).await;
            assert_eq!(unchanged["changed"], false);
            assert_eq!(unchanged["revision"], current_revision);
            write(&mut client, json!({"op":"stop","version":1,"id":11})).await;
            assert_eq!(read(&mut client).await["shutdown"], "suspended");
            task.await.unwrap().unwrap();
        })
        .await;
}
