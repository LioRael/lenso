use std::time::Duration;

use lenso_native_adapter::CancellationToken;

use crate::{Plugin, state::Notification};

impl Plugin {
    pub(crate) fn spawn(&self, id: usize) -> Result<(), &'static str> {
        let ready = self
            .lifecycle
            .readiness()
            .map_err(|_| "generation_stopped")?;
        let generation = ready.cancellation();
        let cancellation = CancellationToken::new();
        self.cancellations
            .borrow_mut()
            .insert(id, cancellation.clone());
        let owner = self.clone();
        let lifetime = TaskLifetime {
            owner: self.clone(),
            id,
        };
        {
            let mut probe = self.runtime.probe.0.borrow_mut();
            probe.started += 1;
            probe.active += 1;
        }
        self.lifecycle
            .spawn_local(async move {
                let _lifetime = lifetime;
                tokio::select! {
                    biased;
                    () = generation.cancelled() => {},
                    () = cancellation.cancelled() => {},
                    () = async { ready.wait().await; owner.run(id).await; } => {},
                }
            })
            .map(|_| ())
            .map_err(|_| "generation_stopped")
    }

    async fn run(&self, id: usize) {
        if self.run_job(id).await.is_err() {
            println!(
                "{}",
                serde_json::json!({"event": "job_storage_failed", "job_id": id})
            );
        }
    }

    async fn run_job(&self, id: usize) -> Result<(), &'static str> {
        let job = self.store.borrow().state.jobs[id - 1].clone();
        let delay = if job.status == "retrying" {
            100 * u64::from(job.attempts)
        } else {
            self.store
                .borrow_mut()
                .commit(|state| state.jobs[id - 1].status = "running".into())?;
            job.delay_ms
        };
        self.runtime.sleep(Duration::from_millis(delay)).await;
        loop {
            self.store.borrow().healthy()?;
            let job = self.store.borrow().state.jobs[id - 1].clone();
            if job.fail || job.attempts >= 3 {
                self.finish_failed(
                    id,
                    if job.fail {
                        "requested_failure"
                    } else {
                        "delivery_failed"
                    },
                )?;
                return Ok(());
            }
            let attempt = job.attempts + 1;
            self.store.borrow_mut().commit(|state| {
                state.jobs[id - 1].status = "running".into();
                state.jobs[id - 1].attempts = attempt;
            })?;
            self.runtime
                .probe
                .0
                .borrow_mut()
                .delivery_attempts
                .push((id, attempt));
            if (self.runtime.before_delivery)(id, attempt).await.is_err() {
                if attempt == 3 {
                    self.finish_failed(id, "delivery_failed")?;
                    return Ok(());
                }
                self.store.borrow_mut().commit(|state| {
                    state.jobs[id - 1].status = "retrying".into();
                    state.jobs[id - 1].error = Some("delivery_failed".into());
                })?;
                self.runtime
                    .sleep(Duration::from_millis(100 * u64::from(attempt)))
                    .await;
                continue;
            }
            self.store.borrow_mut().commit(|state| {
                let job = &mut state.jobs[id - 1];
                job.status = "completed".into();
                job.error = None;
                state.notifications.push(Notification {
                    job_id: id,
                    message: job.message.clone(),
                });
            })?;
            println!(
                "{}",
                serde_json::json!({"event": "job_finished", "job_id": id, "status": "completed"})
            );
            let receipt = (self.runtime.after_commit)(id).await;
            let mut probe = self.runtime.probe.0.borrow_mut();
            if receipt.is_ok() {
                probe.receipts += 1;
            } else {
                probe.receipt_failures += 1;
            }
            return Ok(());
        }
    }

    fn finish_failed(&self, id: usize, error: &str) -> Result<(), &'static str> {
        self.store.borrow_mut().commit(|state| {
            state.jobs[id - 1].status = "failed".into();
            state.jobs[id - 1].error = Some(error.into());
        })?;
        println!(
            "{}",
            serde_json::json!({"event": "job_finished", "job_id": id, "status": "failed"})
        );
        Ok(())
    }
}

struct TaskLifetime {
    owner: Plugin,
    id: usize,
}

impl Drop for TaskLifetime {
    fn drop(&mut self) {
        let cancelled = self.owner.store.borrow_mut().interrupted(self.id);
        self.owner.cancellations.borrow_mut().remove(&self.id);
        let mut probe = self.owner.runtime.probe.0.borrow_mut();
        probe.active -= 1;
        probe.dropped += 1;
        probe.dropped_job_ids.push(self.id);
        if cancelled {
            probe.shutdown_cancelled_job_ids.push(self.id);
        }
        println!(
            "{}",
            serde_json::json!({"event": "task_dropped", "job_id": self.id})
        );
    }
}
