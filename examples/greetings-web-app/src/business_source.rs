use std::{
    fs,
    io::Read as _,
    net::SocketAddr,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, ensure};
use lenso_engine_authoring::{
    BusinessRequestSnapshot, BusinessSnapshotAcceptance, BusinessSnapshotAuthority,
    BusinessSnapshotAuthorization, BusinessSnapshotCursor, BusinessSnapshotObjectId,
    BusinessSnapshotPoll, BusinessSnapshotSourceBinding, BusinessSnapshotSourceId,
    FileBusinessSnapshotSource, HttpsBusinessSnapshotSource, VersionedBusinessSnapshot,
};
use lenso_web_greetings_plugin_example::{
    GreetingPolicy, GreetingPolicySource, GreetingPolicyUnavailable, GreetingsHttp,
    PinnedGreetingPolicy,
};
use lenso_web_host::{NativeWebHost, RunningNativeWebHost};
use serde::Deserialize;
use tokio::{task::JoinHandle, time::MissedTickBehavior};

const POLICY_SCHEMA: &str = "lenso.example-greeting-business-policy.v1";
const OBJECT_KEY: &str = "greeting-policy";
const MAX_POLICY_BYTES: u64 = 64 * 1024;
const POLICY_CHECK_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    source_reference: String,
    source: SourceSpec,
    max_stale_seconds: u64,
    poll_millis: u64,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum SourceSpec {
    File {
        path: PathBuf,
    },
    Https {
        url: String,
        admitted_origins: Vec<String>,
    },
}

enum Source {
    File(FileBusinessSnapshotSource),
    Https(HttpsBusinessSnapshotSource),
}

enum Observation {
    File(VersionedBusinessSnapshot),
    Https(BusinessSnapshotPoll),
}

impl Source {
    fn binding(&self) -> BusinessSnapshotSourceBinding {
        match self {
            Self::File(source) => source.binding(),
            Self::Https(source) => source.binding(),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::File(_) => "file",
            Self::Https(_) => "https_poll",
        }
    }

    fn observe(&self, cursor: Option<&BusinessSnapshotCursor>) -> anyhow::Result<Observation> {
        match self {
            Self::File(source) => Ok(Observation::File(source.read()?)),
            Self::Https(source) => Ok(Observation::Https(source.poll(cursor)?)),
        }
    }
}

struct AuthorizedGreetingPolicy {
    authority: Arc<BusinessSnapshotAuthority<GreetingPolicy>>,
}

impl GreetingPolicySource for AuthorizedGreetingPolicy {
    fn capture(&self) -> Result<PinnedGreetingPolicy, GreetingPolicyUnavailable> {
        let pinned: BusinessRequestSnapshot<GreetingPolicy> = self
            .authority
            .capture_request()
            .map_err(|_| GreetingPolicyUnavailable)?;
        Ok(PinnedGreetingPolicy {
            revision: pinned.revision(),
            value: pinned.value().clone(),
        })
    }
}

struct HostBusinessSource {
    policy_path: PathBuf,
    accepted_policy: Vec<u8>,
    source: Arc<Source>,
    authority: Arc<BusinessSnapshotAuthority<GreetingPolicy>>,
    poll_interval: Duration,
}

impl HostBusinessSource {
    fn bootstrap(policy_path: PathBuf) -> anyhow::Result<Self> {
        let accepted_policy = read_policy(&policy_path)
            .map_err(|_| anyhow::anyhow!("business policy is unavailable or invalid"))?;
        let policy: Policy = serde_json::from_slice(&accepted_policy)
            .map_err(|_| anyhow::anyhow!("business policy is unavailable or invalid"))?;
        ensure!(
            policy.schema == POLICY_SCHEMA,
            "unsupported business policy schema"
        );
        ensure!(
            (1..=86_400).contains(&policy.max_stale_seconds),
            "business policy stale limit is outside 1..=86400 seconds"
        );
        ensure!(
            (100..=60_000).contains(&policy.poll_millis)
                && u128::from(policy.poll_millis) <= u128::from(policy.max_stale_seconds) * 500,
            "business policy poll interval exceeds half its freshness limit"
        );
        let source = match policy.source {
            SourceSpec::File { path } => {
                ensure!(
                    path.is_absolute(),
                    "business source file path must be absolute"
                );
                let id = BusinessSnapshotSourceId::new("file", policy.source_reference)?;
                Source::File(FileBusinessSnapshotSource::new(path, id))
            }
            SourceSpec::Https {
                url,
                admitted_origins,
            } => {
                let id = BusinessSnapshotSourceId::new("https_poll", policy.source_reference)?;
                Source::Https(HttpsBusinessSnapshotSource::new(
                    &url,
                    id,
                    &admitted_origins,
                )?)
            }
        };
        let mut schema: serde_json::Value = serde_json::from_str(
            lenso_web_greetings_plugin_example::__lenso_config_schema_greeting_policy!(),
        )?;
        ensure!(
            schema.get("$schema").and_then(serde_json::Value::as_str)
                == Some("https://json-schema.org/draft/2020-12/schema"),
            "business snapshot schema dialect is unsupported"
        );
        // Host ceilings accept the generated Plugin schema's constraints, not its dialect tag.
        schema
            .as_object_mut()
            .expect("Plugin schema is an object")
            .remove("$schema");
        let authorization = BusinessSnapshotAuthorization::new(
            BusinessSnapshotObjectId::new("company.greetings-http", "default", OBJECT_KEY)?,
            source.binding(),
            schema,
            ["exclamation_count"],
            Duration::from_secs(policy.max_stale_seconds),
        )?;
        let authority = Arc::new(BusinessSnapshotAuthority::new(authorization));
        let initial = source
            .observe(None)
            .map_err(|_| anyhow::anyhow!("business snapshot initial source was rejected"))?;
        Self::accept(&authority, initial, None)
            .map_err(|_| anyhow::anyhow!("business snapshot initial value was rejected"))?;
        let source = Arc::new(source);
        eprintln!(
            "business_snapshot code=activated source_kind={} revision={}",
            source.kind(),
            authority
                .active_revision()?
                .context("missing initial revision")?
        );
        Ok(Self {
            policy_path,
            accepted_policy,
            source,
            authority,
            poll_interval: Duration::from_millis(policy.poll_millis),
        })
    }

    fn accept(
        authority: &BusinessSnapshotAuthority<GreetingPolicy>,
        observation: Observation,
        expected: Option<u64>,
    ) -> anyhow::Result<BusinessSnapshotAcceptance> {
        match observation {
            Observation::File(snapshot) => authority.accept(snapshot, expected),
            Observation::Https(poll) => authority.accept_poll(poll, expected),
        }
    }

    fn reader(&self) -> Rc<dyn GreetingPolicySource> {
        Rc::new(AuthorizedGreetingPolicy {
            authority: Arc::clone(&self.authority),
        })
    }

    fn policy_unchanged(&self) -> bool {
        read_policy(&self.policy_path).is_ok_and(|bytes| bytes == self.accepted_policy)
    }

    fn spawn_observation(
        &self,
    ) -> anyhow::Result<(JoinHandle<anyhow::Result<Observation>>, Option<u64>)> {
        let source = Arc::clone(&self.source);
        let cursor = self.authority.cursor()?;
        let expected = self.authority.active_revision()?;
        Ok((
            tokio::task::spawn_blocking(move || source.observe(cursor.as_ref())),
            expected,
        ))
    }

    fn accept_observation(
        &self,
        observation: Observation,
        expected: Option<u64>,
    ) -> anyhow::Result<BusinessSnapshotAcceptance> {
        Self::accept(&self.authority, observation, expected)
    }
}

fn read_policy(path: &Path) -> anyhow::Result<Vec<u8>> {
    ensure!(path.is_absolute(), "business policy path must be absolute");
    let descriptor = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::CLOEXEC | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    )?;
    let file = fs::File::from(descriptor);
    let metadata = file.metadata()?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= MAX_POLICY_BYTES,
        "business policy must be a bounded regular file"
    );
    let mut bytes = Vec::new();
    (&file).take(MAX_POLICY_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= MAX_POLICY_BYTES,
        "business policy exceeds its size bound"
    );
    Ok(bytes)
}

/// Runs one explicitly configured Native Host. This is not the generated App Host path.
pub async fn run(
    policy_path: PathBuf,
    bind: SocketAddr,
    ready: Option<tokio::sync::oneshot::Sender<SocketAddr>>,
) -> anyhow::Result<()> {
    let source = HostBusinessSource::bootstrap(policy_path)?;
    ensure!(
        source.policy_unchanged(),
        "business policy changed before Host startup"
    );
    let reader = source.reader();
    let running = NativeWebHost::new()
        .configured_plugin::<GreetingsHttp, _>(move |plugin| {
            plugin.bind_business_policy(Rc::clone(&reader));
            Ok(())
        })
        .bind(bind)
        .start()
        .await
        .map_err(|_| anyhow::anyhow!("business Host failed before readiness"))?;
    if !source.policy_unchanged() {
        running
            .shutdown()
            .await
            .context("stop revoked business Host")?;
        return Err(anyhow::anyhow!("business policy changed before readiness"));
    }
    let address = running.address();
    if let Some(ready) = ready {
        let _ = ready.send(address);
    }
    eprintln!(
        "business_snapshot code=host_ready source_kind={}",
        source.source.kind()
    );
    supervise(running, source).await
}

async fn supervise(
    running: RunningNativeWebHost,
    source: HostBusinessSource,
) -> anyhow::Result<()> {
    let mut poll = tokio::time::interval_at(
        tokio::time::Instant::now() + source.poll_interval,
        source.poll_interval,
    );
    poll.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut policy_check = tokio::time::interval_at(
        tokio::time::Instant::now() + POLICY_CHECK_INTERVAL,
        POLICY_CHECK_INTERVAL,
    );
    let mut pending: Option<(JoinHandle<anyhow::Result<Observation>>, Option<u64>)> = None;
    let mut last_failure: Option<&'static str> = None;
    let outcome = loop {
        tokio::select! {
            biased;
            _ = policy_check.tick() => {
                if !source.policy_unchanged() {
                    eprintln!("business_snapshot code=policy_revoked");
                    break Err(anyhow::anyhow!("business policy changed or became unavailable"));
                }
                if source.authority.capture_request().is_err() {
                    eprintln!("business_snapshot code=source_expired");
                    break Err(anyhow::anyhow!("business snapshot freshness expired"));
                }
            }
            result = async { (&mut pending.as_mut().expect("poll exists").0).await }, if pending.is_some() => {
                let (_, expected) = pending.take().expect("completed poll exists");
                if let Ok(Ok(observation)) = result {
                    if let Ok(acceptance) = source.accept_observation(observation, expected) {
                        last_failure = None;
                        let Ok(Some(revision)) = source.authority.active_revision() else {
                            break Err(anyhow::anyhow!("active business revision is unavailable"));
                        };
                        if acceptance == BusinessSnapshotAcceptance::Activated {
                            eprintln!("business_snapshot code=activated source_kind={} revision={revision}", source.source.kind());
                        }
                    } else {
                        if last_failure != Some("refresh_rejected") {
                            eprintln!("business_snapshot code=refresh_rejected source_kind={}", source.source.kind());
                        }
                        last_failure = Some("refresh_rejected");
                    }
                } else {
                    if last_failure != Some("source_unavailable") {
                        eprintln!("business_snapshot code=source_unavailable source_kind={}", source.source.kind());
                    }
                    last_failure = Some("source_unavailable");
                }
            }
            _ = poll.tick(), if pending.is_none() => {
                match source.spawn_observation() {
                    Ok(observation) => pending = Some(observation),
                    Err(_) => break Err(anyhow::anyhow!("business observation could not start")),
                }
            }
            signal = tokio::signal::ctrl_c() => {
                break signal.context("watch Host shutdown signal");
            }
        }
    };
    running.shutdown().await.context("stop business Host")?;
    outcome
}

#[cfg(test)]
mod tests {
    use std::{fs, net::SocketAddr, path::Path, time::Duration};

    use serde_json::{Value, json};
    use tokio::{
        io::{AsyncReadExt as _, AsyncWriteExt as _},
        net::TcpStream,
        sync::oneshot,
        task::LocalSet,
    };

    use super::{HostBusinessSource, run};

    fn write_snapshot(path: &Path, revision: u64, count: u64) {
        let next = path.with_extension("next");
        fs::write(
            &next,
            json!({
                "schema": "lenso.business-snapshot.v1",
                "object": {
                    "plugin_id": "company.greetings-http",
                    "instance_key": "default",
                    "object_key": "greeting-policy"
                },
                "revision": revision,
                "value": { "exclamation_count": count }
            })
            .to_string(),
        )
        .unwrap();
        fs::rename(next, path).unwrap();
    }

    fn write_policy(path: &Path, snapshot: &Path, max_stale_seconds: u64) {
        fs::write(
            path,
            json!({
                "schema": "lenso.example-greeting-business-policy.v1",
                "source_reference": "greeting-operator",
                "source": {"type": "file", "path": snapshot},
                "max_stale_seconds": max_stale_seconds,
                "poll_millis": 100
            })
            .to_string(),
        )
        .unwrap();
    }

    async fn create(address: SocketAddr) -> (u16, Value) {
        let mut connection = TcpStream::connect(address).await.unwrap();
        let body = r#"{"name":"Ada"}"#;
        let wire = format!(
            "POST /greetings HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        connection.write_all(wire.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        connection.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap();
        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        (
            head.split_whitespace().nth(1).unwrap().parse().unwrap(),
            serde_json::from_str(body).unwrap(),
        )
    }

    async fn wait_for_revision(address: SocketAddr, revision: u64) -> Value {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let (status, body) = create(address).await;
                if status == 201 && body["policy_revision"] == revision {
                    return body;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn real_native_http_uses_new_accepted_revision_and_stops_after_invalid_source_expires() {
        LocalSet::new()
            .run_until(async {
                let directory = tempfile::tempdir().unwrap();
                let snapshot = directory.path().join("business.json");
                let policy = directory.path().join("operator-policy.json");
                write_snapshot(&snapshot, 1, 1);
                write_policy(&policy, &snapshot, 2);

                let (ready, address) = oneshot::channel();
                let host = tokio::task::spawn_local(run(
                    policy,
                    SocketAddr::from(([127, 0, 0, 1], 0)),
                    Some(ready),
                ));
                let Ok(address) = address.await else {
                    panic!("Host failed before readiness: {:?}", host.await);
                };
                let first = wait_for_revision(address, 1).await;
                assert_eq!(first["message"], "Hello, Ada!");

                write_snapshot(&snapshot, 2, 3);
                let next = wait_for_revision(address, 2).await;
                assert_eq!(next["message"], "Hello, Ada!!!");

                write_snapshot(&snapshot, 3, 300);
                tokio::time::sleep(Duration::from_millis(250)).await;
                let (status, still_active) = create(address).await;
                assert_eq!(status, 201);
                assert_eq!(still_active["policy_revision"], 2);
                assert_eq!(still_active["message"], "Hello, Ada!!!");

                let stopped = tokio::time::timeout(Duration::from_secs(3), host)
                    .await
                    .expect("expired source must stop the Host")
                    .unwrap();
                assert!(stopped.is_err());
                assert!(TcpStream::connect(address).await.is_err());
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn changed_host_policy_revokes_the_running_native_http_service() {
        LocalSet::new()
            .run_until(async {
                let directory = tempfile::tempdir().unwrap();
                let snapshot = directory.path().join("business.json");
                let policy = directory.path().join("operator-policy.json");
                write_snapshot(&snapshot, 1, 2);
                write_policy(&policy, &snapshot, 10);
                let (ready, address) = oneshot::channel();
                let host = tokio::task::spawn_local(run(
                    policy.clone(),
                    SocketAddr::from(([127, 0, 0, 1], 0)),
                    Some(ready),
                ));
                let Ok(address) = address.await else {
                    panic!("Host failed before readiness: {:?}", host.await);
                };
                assert_eq!(
                    wait_for_revision(address, 1).await["message"],
                    "Hello, Ada!!"
                );

                write_policy(&policy, &snapshot, 11);
                let stopped = tokio::time::timeout(Duration::from_secs(2), host)
                    .await
                    .expect("changed policy must stop the Host")
                    .unwrap();
                assert!(stopped.is_err());
                assert!(TcpStream::connect(address).await.is_err());
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_initial_source_cannot_publish_host_readiness() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = directory.path().join("missing-business.json");
        let policy = directory.path().join("operator-policy.json");
        write_policy(&policy, &snapshot, 2);
        assert!(HostBusinessSource::bootstrap(policy.clone()).is_err());
        let (ready, address) = oneshot::channel();
        assert!(
            LocalSet::new()
                .run_until(run(
                    policy,
                    SocketAddr::from(([127, 0, 0, 1], 0)),
                    Some(ready)
                ))
                .await
                .is_err()
        );
        assert!(address.await.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invalid_initial_value_cannot_publish_host_readiness() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = directory.path().join("business.json");
        let policy = directory.path().join("operator-policy.json");
        write_snapshot(&snapshot, 1, 300);
        write_policy(&policy, &snapshot, 2);
        let (ready, address) = oneshot::channel();
        assert!(
            LocalSet::new()
                .run_until(run(
                    policy,
                    SocketAddr::from(([127, 0, 0, 1], 0)),
                    Some(ready)
                ))
                .await
                .is_err()
        );
        assert!(address.await.is_err());
    }
}
