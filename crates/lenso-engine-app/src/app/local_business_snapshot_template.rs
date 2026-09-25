use std::{
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, bail, ensure};
use lenso_app_plan::ResolvedAppPlan;
use lenso_engine_authoring::{
    BusinessSnapshotAuthority, BusinessSnapshotAuthorization, BusinessSnapshotObjectId,
    BusinessSnapshotSourceId, FileBusinessSnapshotSource, HttpsBusinessSnapshotSource,
};
use lenso_native_adapter::{ConfiguredPluginFactory, NativePluginRegistry};
use serde::Deserialize;

use __KNOWLEDGE_PLUGIN_CRATE__::{
    AttachmentPolicy, AttachmentPolicySource, AttachmentPolicyUnavailable, KnowledgeBase,
    PinnedAttachmentPolicy, attachment_policy_schema,
};

const PLUGIN_ID: &str = "lenso.reference.knowledge-base";
const INSTANCE_KEY: &str = "default";
const OBJECT_KEY: &str = "attachment-policy";
const MAX_POLICY_BYTES: u64 = 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    object: BusinessSnapshotObjectId,
    source: SourcePolicy,
    poll_interval_millis: u64,
    max_stale_millis: u64,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum SourcePolicy {
    File {
        reference: String,
        path: PathBuf,
    },
    Https {
        reference: String,
        url: String,
        admitted_origins: Vec<String>,
    },
}

enum Source {
    File(FileBusinessSnapshotSource),
    Https(HttpsBusinessSnapshotSource),
}

impl Source {
    fn update(
        &self,
        authority: &BusinessSnapshotAuthority<AttachmentPolicy>,
    ) -> anyhow::Result<()> {
        let previous = authority.active_revision()?;
        match self {
            Self::File(source) => {
                authority.accept(source.read()?, previous)?;
            }
            Self::Https(source) => {
                let cursor = authority.cursor()?;
                authority.accept_poll(source.poll(cursor.as_ref())?, previous)?;
            }
        }
        Ok(())
    }
}

struct AttachmentPolicyView {
    authority: Arc<BusinessSnapshotAuthority<AttachmentPolicy>>,
    available: Arc<AtomicBool>,
}

impl AttachmentPolicySource for AttachmentPolicyView {
    fn capture(&self) -> Result<PinnedAttachmentPolicy, AttachmentPolicyUnavailable> {
        if !self.available.load(Ordering::Acquire) {
            return Err(AttachmentPolicyUnavailable);
        }
        let pinned = self
            .authority
            .capture_request()
            .map_err(|_| AttachmentPolicyUnavailable)?;
        Ok(PinnedAttachmentPolicy {
            revision: pinned.revision(),
            value: pinned.value().clone(),
        })
    }
}

pub struct Poller {
    source: Arc<Source>,
    authority: Arc<BusinessSnapshotAuthority<AttachmentPolicy>>,
    available: Arc<AtomicBool>,
    interval: Duration,
}

impl Poller {
    pub async fn recheck(&self) -> anyhow::Result<()> {
        let source = Arc::clone(&self.source);
        let authority = Arc::clone(&self.authority);
        let result = tokio::task::spawn_blocking(move || source.update(&authority))
            .await
            .context("business snapshot poll task failed")
            .and_then(|result| result);
        self.available.store(result.is_ok(), Ordering::Release);
        result.context("authorized attachment policy source is unavailable")
    }

    pub fn spawn(self) -> PollerGuard {
        let available = Arc::clone(&self.available);
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.interval);
            interval.tick().await;
            loop {
                interval.tick().await;
                let was_available = self.available.load(Ordering::Acquire);
                let result = self.recheck().await;
                if was_available && result.is_err() {
                    eprintln!("authorized attachment policy source is unavailable");
                } else if !was_available && result.is_ok() {
                    eprintln!("authorized attachment policy source recovered");
                }
            }
        });
        PollerGuard { available, task }
    }
}

pub struct PollerGuard {
    available: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for PollerGuard {
    fn drop(&mut self) {
        self.available.store(false, Ordering::Release);
        self.task.abort();
    }
}

pub fn bind(
    registry: NativePluginRegistry,
    plan: &ResolvedAppPlan,
    policy_path: &Path,
) -> anyhow::Result<(NativePluginRegistry, Poller)> {
    let selected = plan
        .plugin_instances()
        .iter()
        .filter(|instance| instance.package_id() == PLUGIN_ID)
        .collect::<Vec<_>>();
    ensure!(
        selected.len() == 1
            && selected[0].instance_key() == INSTANCE_KEY
            && selected[0].execution_class().as_str() == "lenso.native-rust@1",
        "attachment policy requires the exact selected linked KnowledgeBase default Instance"
    );
    let policy = read_policy(policy_path)?;
    let expected = authorize_policy(&policy)?;
    let source = match policy.source {
        SourcePolicy::File { reference, path } => {
            ensure!(
                path.is_absolute(),
                "attachment policy file path must be absolute"
            );
            Source::File(FileBusinessSnapshotSource::new(
                path,
                BusinessSnapshotSourceId::new("file", reference)?,
            ))
        }
        SourcePolicy::Https {
            reference,
            url,
            admitted_origins,
        } => Source::Https(HttpsBusinessSnapshotSource::new(
            &url,
            BusinessSnapshotSourceId::new("https", reference)?,
            &admitted_origins,
        )?),
    };
    let binding = match &source {
        Source::File(source) => source.binding(),
        Source::Https(source) => source.binding(),
    };
    let authorization = BusinessSnapshotAuthorization::new(
        expected,
        binding,
        attachment_policy_schema(),
        ["max_attachment_bytes"],
        Duration::from_millis(policy.max_stale_millis),
    )?;
    let authority = Arc::new(BusinessSnapshotAuthority::<AttachmentPolicy>::new(
        authorization,
    ));
    source
        .update(&authority)
        .context("initial authorized attachment policy source is unavailable")?;
    let available = Arc::new(AtomicBool::new(true));
    let view: Rc<dyn AttachmentPolicySource> = Rc::new(AttachmentPolicyView {
        authority: Arc::clone(&authority),
        available: Arc::clone(&available),
    });
    let registry = registry
        .with_factory_override(ConfiguredPluginFactory::<KnowledgeBase, _>::new(
            move |plugin| {
                plugin.bind_attachment_policy(Rc::clone(&view));
                Ok(())
            },
        ))
        .map_err(|error| anyhow::anyhow!("attachment policy Host binding failed: {error:?}"))?;
    Ok((
        registry,
        Poller {
            source: Arc::new(source),
            authority,
            available,
            interval: Duration::from_millis(policy.poll_interval_millis),
        },
    ))
}

fn authorize_policy(policy: &Policy) -> anyhow::Result<BusinessSnapshotObjectId> {
    let expected = BusinessSnapshotObjectId::new(PLUGIN_ID, INSTANCE_KEY, OBJECT_KEY)?;
    ensure!(
        policy.schema == "lenso.host-business-snapshot-policy.v1" && policy.object == expected,
        "attachment policy has an unauthorized object or schema"
    );
    ensure!(
        (100..=300_000).contains(&policy.poll_interval_millis)
            && (policy.poll_interval_millis.saturating_mul(2)..=86_400_000)
                .contains(&policy.max_stale_millis),
        "attachment policy poll or stale interval is outside Host bounds"
    );
    Ok(expected)
}

fn read_policy(path: &Path) -> anyhow::Result<Policy> {
    ensure!(
        path.is_absolute(),
        "Host business snapshot policy path must be absolute"
    );
    #[cfg(unix)]
    let file = {
        let descriptor = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::CLOEXEC | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
        )?;
        fs::File::from(descriptor)
    };
    #[cfg(not(unix))]
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= MAX_POLICY_BYTES,
        "Host business snapshot policy must be a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take(MAX_POLICY_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_POLICY_BYTES {
        bail!("Host business snapshot policy exceeds its bound");
    }
    serde_json::from_slice(&bytes).context("Host business snapshot policy is invalid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn policy(object: BusinessSnapshotObjectId) -> Policy {
        Policy {
            schema: "lenso.host-business-snapshot-policy.v1".into(),
            object,
            source: SourcePolicy::File {
                reference: "test-policy".into(),
                path: "/tmp/unused-snapshot.json".into(),
            },
            poll_interval_millis: 100,
            max_stale_millis: 200,
        }
    }

    fn object(instance: &str, key: &str) -> BusinessSnapshotObjectId {
        BusinessSnapshotObjectId::new(PLUGIN_ID, instance, key).unwrap()
    }

    fn document(revision: u64, object: BusinessSnapshotObjectId, bytes: u32) -> Value {
        json!({
            "schema": "lenso.business-snapshot.v1",
            "revision": revision,
            "object": object,
            "value": {"max_attachment_bytes": bytes}
        })
    }

    #[test]
    fn host_policy_rejects_other_instance_object_and_schema() {
        assert!(authorize_policy(&policy(object(INSTANCE_KEY, OBJECT_KEY))).is_ok());
        assert!(authorize_policy(&policy(object("other", OBJECT_KEY))).is_err());
        assert!(authorize_policy(&policy(object(INSTANCE_KEY, "other-policy"))).is_err());
        let mut wrong_schema = policy(object(INSTANCE_KEY, OBJECT_KEY));
        wrong_schema.schema = "lenso.host-business-snapshot-policy.v2".into();
        assert!(authorize_policy(&wrong_schema).is_err());
    }

    #[tokio::test]
    async fn source_loss_and_revision_swap_revoke_until_valid_new_revision() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("attachment-policy.json");
        let source = FileBusinessSnapshotSource::new(
            &path,
            BusinessSnapshotSourceId::new("file", "test-policy").unwrap(),
        );
        let authority = Arc::new(BusinessSnapshotAuthority::new(
            BusinessSnapshotAuthorization::new(
                object(INSTANCE_KEY, OBJECT_KEY),
                source.binding(),
                attachment_policy_schema(),
                ["max_attachment_bytes"],
                Duration::from_secs(10),
            )
            .unwrap(),
        ));
        let available = Arc::new(AtomicBool::new(false));
        let view = AttachmentPolicyView {
            authority: Arc::clone(&authority),
            available: Arc::clone(&available),
        };
        let poller = Poller {
            source: Arc::new(Source::File(source)),
            authority,
            available,
            interval: Duration::from_millis(100),
        };

        assert!(poller.recheck().await.is_err());
        assert!(view.capture().is_err());

        fs::write(
            &path,
            document(1, object(INSTANCE_KEY, OBJECT_KEY), 128).to_string(),
        )
        .unwrap();
        poller.recheck().await.unwrap();
        assert_eq!(view.capture().unwrap().value.max_attachment_bytes, 128);

        fs::remove_file(&path).unwrap();
        assert!(poller.recheck().await.is_err());
        assert!(view.capture().is_err());

        fs::write(
            &path,
            document(2, object("other", OBJECT_KEY), 64).to_string(),
        )
        .unwrap();
        assert!(poller.recheck().await.is_err());
        assert!(view.capture().is_err());

        fs::write(
            &path,
            document(1, object(INSTANCE_KEY, OBJECT_KEY), 64).to_string(),
        )
        .unwrap();
        assert!(poller.recheck().await.is_err());
        assert!(view.capture().is_err());

        fs::write(
            &path,
            document(2, object(INSTANCE_KEY, OBJECT_KEY), 64).to_string(),
        )
        .unwrap();
        poller.recheck().await.unwrap();
        assert_eq!(view.capture().unwrap().revision, 2);
        assert_eq!(view.capture().unwrap().value.max_attachment_bytes, 64);
    }
}
