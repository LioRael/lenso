//! Private application control bridge. The native owner supplies separate physical
//! termination evidence; a suspension receipt alone never proves process exit.

use super::{
    AppGenerationTransitionSpec, CanonicalDocument, ControlPlaneError, Host, ResolvedGeneration,
};
use lenso_plugin_control_plane::{ReplacementMode, RolloutPolicy};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, io, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{mpsc, watch},
};

const LIMIT: usize = 256 * 1024;

#[derive(Clone, Debug)]
pub struct ControlOptions {
    pub distribution: String,
    pub startup_timeout: Duration,
    pub stop_timeout: Duration,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Start {
        version: u32,
        id: u32,
        distribution: String,
    },
    Inspect {
        version: u32,
        id: u32,
        revision: Option<u64>,
        offset: usize,
        limit: usize,
    },
    Reconcile {
        version: u32,
        id: u32,
        revision: u64,
    },
    Stop {
        version: u32,
        id: u32,
    },
}

impl Request {
    fn identity(&self) -> (u32, u32) {
        match self {
            Self::Start { version, id, .. }
            | Self::Inspect { version, id, .. }
            | Self::Reconcile { version, id, .. }
            | Self::Stop { version, id } => (*version, *id),
        }
    }
}

/// Calls the supplied exact runtime assembly only after the distribution handshake.
/// The returned Host must have activated the supplied Generation behind its Ready Gate.
pub async fn serve<R, W, F, Fut, T>(
    options: ControlOptions,
    reader: R,
    writer: W,
    start: F,
) -> io::Result<()>
where
    R: AsyncRead + Unpin + 'static,
    W: AsyncWrite + Unpin,
    T: Clone + std::fmt::Debug + 'static,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(Host<T>, ResolvedGeneration), ControlPlaneError>>,
{
    serve_with_reconcile(options, reader, writer, start, || async {
        Err(ControlPlaneError::HostFailure {
            detail: "this Host does not support reconciliation".to_owned(),
        })
    })
    .await
}

/// Adds an operator-owned candidate resolver to the private control bridge.
/// The controller still performs the exact readiness and fenced route switch.
pub async fn serve_with_reconcile<R, W, F, Fut, C, CFut, T>(
    options: ControlOptions,
    reader: R,
    writer: W,
    start: F,
    reconcile: C,
) -> io::Result<()>
where
    R: AsyncRead + Unpin + 'static,
    W: AsyncWrite + Unpin,
    T: Clone + std::fmt::Debug + 'static,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(Host<T>, ResolvedGeneration), ControlPlaneError>>,
    C: FnMut() -> CFut,
    CFut: Future<Output = Result<ResolvedGeneration, ControlPlaneError>>,
{
    serve_with_reconcile_and_receipt(options, reader, writer, start, reconcile, |_| async {
        Ok(())
    })
    .await
}

/// Runs a Host-owned activation receipt after readiness or a successful route
/// switch. Receipt failure does not undo a healthy active Generation; it is
/// reported explicitly and may be retried by a later reconciliation.
pub async fn serve_with_reconcile_and_receipt<R, W, F, Fut, C, CFut, A, AFut, T>(
    options: ControlOptions,
    reader: R,
    mut writer: W,
    start: F,
    reconcile: C,
    mut receipt: A,
) -> io::Result<()>
where
    R: AsyncRead + Unpin + 'static,
    W: AsyncWrite + Unpin,
    T: Clone + std::fmt::Debug + 'static,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(Host<T>, ResolvedGeneration), ControlPlaneError>>,
    C: FnMut() -> CFut,
    CFut: Future<Output = Result<ResolvedGeneration, ControlPlaneError>>,
    A: FnMut(&ResolvedGeneration) -> AFut,
    AFut: Future<Output = Result<(), ControlPlaneError>>,
{
    if options.startup_timeout.is_zero()
        || options.stop_timeout.is_zero()
        || options.startup_timeout > Duration::from_secs(60)
        || options.stop_timeout > Duration::from_secs(60)
        || options.distribution.is_empty()
        || options.distribution.len() > 256
    {
        return Err(io::Error::other("invalid application control limits"));
    }
    let (send, mut requests) = mpsc::channel(8);
    let (stop, mut stopping) = watch::channel(None);
    let input = tokio::task::spawn_local(read_requests(reader, send, stop));
    let _input = InputTask(input);
    let deadline = tokio::time::Instant::now() + options.startup_timeout;
    let request = tokio::time::timeout_at(deadline, requests.recv())
        .await
        .ok()
        .flatten();
    if !matches!(request, Some(Request::Start { version: 1, id: 1, ref distribution }) if distribution == &options.distribution)
    {
        return emit(
            &mut writer,
            &json!({"kind":"start_failed","version":1,"id":1,"cause":"invalid_start"}),
            options.stop_timeout,
        )
        .await;
    }
    let started = tokio::select! {
        biased;
        _ = stopping.changed() => None,
        result = tokio::time::timeout_at(deadline, start()) => result.ok().and_then(Result::ok),
    };
    let Some((mut host, generation)) = started else {
        return emit(&mut writer, &json!({"kind":"start_failed","version":1,"id":1,"cause":"startup_failed_or_cancelled"}), options.stop_timeout).await;
    };
    let ready = host.inspect().await;
    let valid = ready.as_ref().is_ok_and(|state| {
        !state.host_suspended
            && state.active_generation_spec_digest.as_deref() == Some(generation.spec.digest())
    });
    if !valid {
        let _ = host.drain_and_suspend(options.stop_timeout).await;
        return emit(
            &mut writer,
            &json!({"kind":"start_failed","version":1,"id":1,"cause":"generation_not_ready"}),
            options.stop_timeout,
        )
        .await;
    }
    let state = ready.map_err(io::Error::other)?;
    let revision = state.revision;
    let activation_recorded = tokio::time::timeout(options.startup_timeout, receipt(&generation))
        .await
        .is_ok_and(|result| result.is_ok());
    let announced = async {
        emit(&mut writer, &json!({"kind":"ready","version":1,"revision":revision}), options.stop_timeout).await?;
        emit(&mut writer, &json!({"kind":"started","version":1,"id":1,"revision":revision,"distribution":options.distribution,"activation_recorded":activation_recorded}), options.stop_timeout).await
    }.await;
    if announced.is_err() {
        let _ = host.drain_and_suspend(options.stop_timeout).await;
        return announced;
    }
    let result = session(
        &options,
        &mut host,
        generation,
        reconcile,
        receipt,
        &mut requests,
        &mut stopping,
        &mut writer,
    )
    .await;
    if result.is_err() {
        let _ = host.drain_and_suspend(options.stop_timeout).await;
    }
    result
}

#[expect(clippy::too_many_arguments, reason = "private control session requires independent IO and Host callbacks")]
async fn session<W, C, CFut, A, AFut, T>(
    options: &ControlOptions,
    host: &mut Host<T>,
    mut generation: ResolvedGeneration,
    mut reconcile: C,
    mut receipt: A,
    requests: &mut mpsc::Receiver<Request>,
    stopping: &mut watch::Receiver<Option<u32>>,
    writer: &mut W,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
    T: Clone + std::fmt::Debug + 'static,
    C: FnMut() -> CFut,
    CFut: Future<Output = Result<ResolvedGeneration, ControlPlaneError>>,
    A: FnMut(&ResolvedGeneration) -> AFut,
    AFut: Future<Output = Result<(), ControlPlaneError>>,
{
    let mut events = host.subscribe();
    loop {
        let stop_id = *stopping.borrow();
        if let Some(id) = stop_id {
            let result = host.drain_and_suspend(options.stop_timeout).await;
            return emit(writer, &json!({"kind":"terminal","version":1,"id":id,"shutdown":if result.is_ok() {"suspended"} else {"failed"}}), options.stop_timeout).await;
        }
        tokio::select! {
            biased;
            _ = stopping.changed() => {},
            event = events.recv() => {
                if matches!(event, Err(_) | Ok(super::GenerationControllerEvent::Stopped)) {
                    return emit(writer, &json!({"kind":"terminal","version":1,"id":0,"shutdown":"failed"}), options.stop_timeout).await;
                }
            }
            request = requests.recv() => {
                match request {
                    Some(Request::Inspect { id, revision: expected, offset, limit, .. }) => {
                        let current = host.inspect().await.map_err(io::Error::other)?;
                        if current.active_generation_spec_digest.as_deref() != Some(generation.spec.digest()) {
                            emit(writer, &json!({"kind":"rejected","version":1,"id":id,"code":"active_generation_changed"}), options.stop_timeout).await?;
                            continue;
                        }
                        let revision = current.revision;
                        let instances = generation.plan.plugin_instances();
                        if expected.is_some_and(|expected| expected != revision) || limit == 0 || limit > 64 || offset > instances.len() {
                            emit(writer, &json!({"kind":"rejected","version":1,"id":id,"code":"snapshot_expired_or_invalid_page"}), options.stop_timeout).await?;
                            continue;
                        }
                        let end = offset.saturating_add(limit).min(instances.len());
                        let page = instances[offset..end].iter().map(|instance| {
                            let artifacts = generation.artifact_set.value().artifacts.iter().filter(|artifact| artifact.plugin_id == instance.package_id()).map(|artifact| json!({"id":artifact.artifact_id,"digest":artifact.digest,"target":artifact.target})).collect::<Vec<_>>();
                            json!({"instance":instance.instance_key(),"plugin":instance.package_id(),"package_revision":instance.package_revision(),"execution_class":instance.execution_class().as_str(),"artifacts":artifacts})
                        }).collect::<Vec<_>>();
                        emit(writer, &json!({"kind":"inspected","version":1,"id":id,"revision":revision,"generation":generation.spec.digest(),"instances":page,"diagnostics":[],"next":if end < instances.len() { Some(end) } else { None }}), options.stop_timeout).await?;
                    }
                    Some(Request::Reconcile { id, revision: expected, .. }) => {
                        let current = host.inspect().await.map_err(io::Error::other)?;
                        if current.active_generation_spec_digest.as_deref() != Some(generation.spec.digest()) {
                            emit(writer, &json!({"kind":"rejected","version":1,"id":id,"code":"active_generation_changed"}), options.stop_timeout).await?;
                            continue;
                        }
                        let revision = current.revision;
                        if expected != revision {
                            emit(writer, &json!({"kind":"rejected","version":1,"id":id,"code":"snapshot_expired"}), options.stop_timeout).await?;
                            continue;
                        }
                        let Ok(Ok(candidate)) = tokio::time::timeout(options.startup_timeout, reconcile()).await else {
                            emit(writer, &json!({"kind":"rejected","version":1,"id":id,"code":"candidate_unavailable"}), options.stop_timeout).await?;
                            continue;
                        };
                        if candidate.spec.digest() == generation.spec.digest() {
                            let activation_recorded = tokio::time::timeout(options.startup_timeout, receipt(&generation)).await.is_ok_and(|result| result.is_ok());
                            emit(writer, &json!({"kind":"reconciled","version":1,"id":id,"revision":revision,"generation":generation.spec.digest(),"changed":false,"activation_recorded":activation_recorded}), options.stop_timeout).await?;
                            continue;
                        }
                        let transition = CanonicalDocument::from_value("lenso-generation-transition.json", AppGenerationTransitionSpec {
                            schema_version: 1,
                            app_id: candidate.spec.value().app_id.clone(),
                            from_generation_spec_digest: Some(generation.spec.digest().to_owned()),
                            to_generation_spec_digest: candidate.spec.digest().to_owned(),
                            replacement_mode: ReplacementMode::Overlap,
                            state_compatibility_receipt_digests: Vec::new(),
                            rollout_policy: RolloutPolicy {
                                ready_timeout_nanos: options.startup_timeout.as_nanos().to_string(),
                                drain_timeout_nanos: options.stop_timeout.as_nanos().to_string(),
                                rollback_window_nanos: "0".to_owned(),
                                automatic_rollback_on_generation_failure: false,
                            },
                        }).map_err(io::Error::other)?;
                        if host.transition(transition, candidate.clone(), BTreeMap::default()).await.is_err() {
                            let still_active = host.inspect().await.is_ok_and(|state| {
                                state.active_generation_spec_digest.as_deref() == Some(generation.spec.digest())
                                    && !state.host_suspended
                            });
                            if !still_active {
                                emit(writer, &json!({"kind":"terminal","version":1,"id":id,"shutdown":"failed"}), options.stop_timeout).await?;
                                return Err(io::Error::other("Host lost its active Generation during reconciliation"));
                            }
                            emit(writer, &json!({"kind":"rejected","version":1,"id":id,"code":"candidate_not_ready_or_incompatible"}), options.stop_timeout).await?;
                            continue;
                        }
                        let revision = host.inspect().await.map_err(io::Error::other)?.revision;
                        generation = candidate;
                        let activation_recorded = tokio::time::timeout(options.startup_timeout, receipt(&generation)).await.is_ok_and(|result| result.is_ok());
                        emit(writer, &json!({"kind":"reconciled","version":1,"id":id,"revision":revision,"generation":generation.spec.digest(),"changed":true,"activation_recorded":activation_recorded}), options.stop_timeout).await?;
                    }
                    _ => {
                        let _ = host.drain_and_suspend(options.stop_timeout).await;
                        return Err(io::Error::other("unexpected application operation"));
                    }
                }
            }
        }
    }
}

async fn read_requests(
    mut reader: impl AsyncRead + Unpin,
    requests: mpsc::Sender<Request>,
    stop: watch::Sender<Option<u32>>,
) {
    let mut last = 0;
    loop {
        let request = read(&mut reader).await;
        let Ok(request) = request else {
            stop.send_replace(Some(0));
            break;
        };
        let (version, id) = request.identity();
        if version != 1 || (id <= last && !matches!(request, Request::Stop { id: 0, .. })) {
            stop.send_replace(Some(0));
            break;
        }
        last = id;
        if let Request::Stop { id, .. } = request {
            stop.send_replace(Some(id));
            break;
        }
        if requests.try_send(request).is_err() {
            stop.send_replace(Some(0));
            break;
        }
    }
}

async fn read(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<Request> {
    let length = reader.read_u32().await? as usize;
    if length == 0 || length > LIMIT {
        return Err(io::Error::other("invalid application frame length"));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

async fn emit(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &Value,
    budget: Duration,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > LIMIT {
        return Err(io::Error::other("application frame too large"));
    }
    tokio::time::timeout(budget, async {
        writer
            .write_u32(u32::try_from(bytes.len()).map_err(io::Error::other)?)
            .await?;
        writer.write_all(&bytes).await?;
        writer.flush().await
    })
    .await
    .map_err(|_| io::Error::other("application control write timed out"))?
}

struct InputTask(tokio::task::JoinHandle<()>);
impl Drop for InputTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
