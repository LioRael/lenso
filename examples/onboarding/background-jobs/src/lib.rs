//! Generation-owned background jobs using the real linked Plugin lifecycle.
pub mod health;
mod runtime;
mod state;
mod worker;

pub use runtime::{JobFuture, JobProbe, JobRuntime, ProbeSnapshot};
pub use state::{Job, Notification};

use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

use lenso_capability_http_endpoint::{prelude::*, response::Problem};
use lenso_native_adapter::{CancellationToken, LifecycleContext, RuntimeFailure};
use serde::Deserialize;

use state::Store;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submit {
    message: String,
    delay_ms: u64,
    #[serde(default)]
    fail: bool,
}

#[derive(Debug, Deserialize)]
pub struct JobPath {
    id: usize,
}

#[lenso::plugin(id = "example.background-jobs", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {
    store: Rc<RefCell<Store>>,
    lifecycle: LifecycleContext,
    runtime: JobRuntime,
    cancellations: Rc<RefCell<HashMap<usize, CancellationToken>>>,
}

#[lenso::plugin_impl]
impl Plugin {
    #[create]
    fn create(#[lifecycle] lifecycle: LifecycleContext) -> Self {
        Self {
            store: Rc::new(RefCell::new(Store::default())),
            lifecycle,
            runtime: JobRuntime::default(),
            cancellations: Rc::default(),
        }
    }

    #[stop]
    fn stop(&self) {
        let rejected = self.lifecycle.spawn_local(async {}).is_err();
        self.runtime.probe().0.borrow_mut().stop_rejected += usize::from(rejected);
        println!(
            "{}",
            serde_json::json!({"event": "plugin_stopped", "new_tasks_rejected": rejected})
        );
    }
}

impl Plugin {
    /// Binds private Host infrastructure before readiness. Each generation loads
    /// fresh state; a snapshot path has exactly one owning App at a time.
    pub fn configure(
        &mut self,
        runtime: JobRuntime,
        snapshot_path: Option<PathBuf>,
    ) -> Result<(), RuntimeFailure> {
        self.runtime = runtime;
        self.store = Rc::new(RefCell::new(Store::load(snapshot_path).map_err(failure)?));
        let pending = self
            .store
            .borrow()
            .state
            .jobs
            .iter()
            .filter(|job| !job.terminal())
            .map(|job| job.id)
            .collect::<Vec<_>>();
        for id in pending {
            self.spawn(id).map_err(failure)?;
        }
        Ok(())
    }
}

#[endpoint]
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
impl Plugin {
    #[post("background.submit", "/jobs")]
    async fn submit(
        &self,
        Json(request): Json<Submit>,
    ) -> Result<(StatusCode, Json<Job>), Problem> {
        if request.message.trim().is_empty()
            || request.message.len() > 140
            || !(20..=5000).contains(&request.delay_ms)
        {
            return Err(Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid_job",
                "message must contain 1–140 bytes; delay_ms must be 20–5000",
            ));
        }
        if self.store.borrow().state.jobs.len() >= 128 {
            return Err(Problem::new(
                StatusCode::TOO_MANY_REQUESTS,
                "job_limit",
                "this example retains at most 128 jobs",
            ));
        }
        if self.lifecycle.cancellation().is_cancelled() {
            return Err(unavailable("generation_stopped"));
        }
        let job = Job {
            id: self.store.borrow().state.jobs.len() + 1,
            message: request.message,
            status: "queued".into(),
            error: None,
            attempts: 0,
            delay_ms: request.delay_ms,
            fail: request.fail,
        };
        self.store
            .borrow_mut()
            .commit(|state| state.jobs.push(job.clone()))
            .map_err(unavailable)?;
        if let Err(error) = self.spawn(job.id) {
            self.store.borrow_mut().faulted = true;
            return Err(unavailable(error));
        }
        Ok((StatusCode::ACCEPTED, Json(job)))
    }

    #[delete("background.cancel", "/jobs/{id}")]
    async fn cancel(&self, Path(path): Path<JobPath>) -> Result<Json<Job>, Problem> {
        let job = self
            .store
            .borrow()
            .state
            .jobs
            .iter()
            .find(|j| j.id == path.id)
            .cloned();
        let Some(job) = job else {
            return Err(Problem::new(
                StatusCode::NOT_FOUND,
                "job_not_found",
                "job not found",
            ));
        };
        if !job.terminal() {
            self.store
                .borrow_mut()
                .commit(|state| {
                    let job = &mut state.jobs[path.id - 1];
                    job.status = "cancelled".into();
                    job.error = None;
                })
                .map_err(unavailable)?;
            if let Some(token) = self.cancellations.borrow().get(&path.id) {
                token.cancel();
            }
        }
        self.store.borrow().healthy().map_err(unavailable)?;
        Ok(Json(self.store.borrow().state.jobs[path.id - 1].clone()))
    }

    #[get("background.jobs", "/jobs")]
    async fn jobs(&self) -> Result<Json<Vec<Job>>, Problem> {
        let store = self.store.borrow();
        store.healthy().map_err(unavailable)?;
        Ok(Json(store.state.jobs.clone()))
    }

    #[get("background.notifications", "/notifications")]
    async fn notifications(&self) -> Result<Json<Vec<Notification>>, Problem> {
        let store = self.store.borrow();
        store.healthy().map_err(unavailable)?;
        Ok(Json(store.state.notifications.clone()))
    }
}

fn unavailable(code: &'static str) -> Problem {
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        code,
        "background jobs unavailable",
    )
}

fn failure(detail: &'static str) -> RuntimeFailure {
    RuntimeFailure::PluginFailure {
        detail: detail.into(),
    }
}
