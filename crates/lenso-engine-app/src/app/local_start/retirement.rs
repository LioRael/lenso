//! App-start policy around shared owned-Host retirement evidence.

use anyhow::{Context as _, ensure};

pub(super) use super::super::local_host_retirement::Receipt;
use super::{Active, CrashFence, kill_fenced_group};

pub(super) async fn stop(
    active: &mut Active,
    fence: &mut CrashFence,
    signal: bool,
) -> anyhow::Result<()> {
    let deadline = active.deadline();
    let confirmed = super::super::local_host_retirement::confirm(
        &mut active.child,
        active.group_id,
        active.retirement.as_ref(),
        deadline,
        signal,
    )
    .await;
    if let Err(error) = confirmed {
        kill_fenced_group(&mut active.child, active.group_id, fence).await?;
        return Err(error).context(
            "supervised App was hard-stopped; managed cleanup is unconfirmed; \
             verify all Host descendants before manually removing the recovery fence",
        );
    }
    // Reap only after group inspection, while its PID still reserves the group.
    let exit = active.child.wait().await?;
    ensure!(exit.success(), "supervised Host did not exit cleanly");
    fence.clear()
}
