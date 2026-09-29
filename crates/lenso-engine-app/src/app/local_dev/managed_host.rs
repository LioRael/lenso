//! Development owns a persistent session fence; distributions are temporary.

use super::super::local_host_retirement::{self, CrashFence, Receipt};
use anyhow::{Context as _, ensure};
use std::{
    ops::{Deref, DerefMut},
    path::Path,
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    time::Instant,
};

pub(super) struct Retirement {
    fence: CrashFence,
    receipt: Option<Receipt>,
    deadline: Instant,
}

impl Retirement {
    pub(super) fn fence_session(root: &Path) -> anyhow::Result<()> {
        CrashFence::at(root.join(".lenso/supervised-dev.uncertain"))?.mark()
    }

    pub(super) fn check_session(root: &Path) -> anyhow::Result<()> {
        CrashFence::at(root.join(".lenso/supervised-dev.uncertain"))?;
        Ok(())
    }

    pub(super) fn new(root: &Path, eligible: bool, deadline: Instant) -> anyhow::Result<Self> {
        Ok(Self {
            fence: CrashFence::at(root.join(".lenso/supervised-dev.uncertain"))?,
            receipt: eligible.then(|| Receipt::new(root)).transpose()?,
            deadline,
        })
    }

    pub(super) fn configure(&mut self, command: &mut Command) -> anyhow::Result<()> {
        command.env_remove("LENSO_MANAGED_SHUTDOWN_TOKEN");
        command.env_remove("LENSO_MANAGED_SHUTDOWN_RECEIPT");
        if let Some(receipt) = &self.receipt {
            receipt.configure(command);
        }
        self.fence.mark()
    }

    pub(super) fn clear_unstarted(&mut self) -> anyhow::Result<()> {
        self.fence.clear()
    }
}

pub(super) struct Host {
    child: Child,
    retirement: Option<Retirement>,
}

impl Host {
    pub(super) fn new(child: Child, retirement: Option<Retirement>) -> Self {
        Self { child, retirement }
    }

    pub(super) fn renew(&mut self, deadline: Instant) {
        if let Some(retirement) = &mut self.retirement {
            retirement.deadline = deadline;
        }
    }

    pub(super) async fn retire(&mut self) -> anyhow::Result<()> {
        let Some(retirement) = &mut self.retirement else {
            return super::stop(&mut self.child, false).await;
        };
        let group_id = self
            .child
            .id()
            .context("managed Host was reaped without retirement evidence")?;
        let confirmed = local_host_retirement::confirm(
            &mut self.child,
            group_id,
            retirement.receipt.as_ref(),
            retirement.deadline,
            true,
        )
        .await;
        if let Err(error) = confirmed {
            super::kill_process_group_now(&mut self.child).await?;
            return Err(error).context(
                "managed dev retirement is unconfirmed; the persistent session fence remains",
            );
        }
        let exit = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await??;
        ensure!(
            exit.success(),
            "managed dev Host did not exit cleanly; session remains fenced"
        );
        retirement.fence.clear()
    }
}

impl From<Child> for Host {
    fn from(child: Child) -> Self {
        Self::new(child, None)
    }
}

impl Deref for Host {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}

impl DerefMut for Host {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}
