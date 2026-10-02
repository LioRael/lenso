//! Generation-owned background jobs with notifications kept only in memory.
pub mod health;

use std::{cell::RefCell, rc::Rc, time::Duration};

use lenso_capability_http_endpoint::{prelude::*, response::Problem};
use lenso_native_adapter::LifecycleContext;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub struct Job {
    id: usize,
    message: String,
    status: &'static str,
    error: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Notification {
    job_id: usize,
    message: String,
}

#[derive(Debug, Default)]
struct State {
    jobs: Vec<Job>,
    notifications: Vec<Notification>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submit {
    message: String,
    delay_ms: u64,
    #[serde(default)]
    fail: bool,
}

#[lenso::plugin(id = "example.background-jobs", root_slot = "web")]
#[derive(Clone, Debug)]
pub struct Plugin {
    state: Rc<RefCell<State>>,
    lifecycle: LifecycleContext,
}

#[lenso::plugin_impl]
impl Plugin {
    #[create]
    fn create(#[lifecycle] lifecycle: LifecycleContext) -> Self {
        Self {
            state: Rc::new(RefCell::new(State::default())),
            lifecycle,
        }
    }

    #[stop]
    fn stop(&self) {
        // Probe the retained construction context: its generation scope is closed.
        let rejected = self.lifecycle.spawn_local(async {}).is_err();
        println!(
            "{}",
            serde_json::json!({"event": "plugin_stopped", "new_tasks_rejected": rejected})
        );
    }
}

/// Observable task ownership: dropping the future releases its generation work.
struct TaskLifetime(usize);

impl Drop for TaskLifetime {
    fn drop(&mut self) {
        println!(
            "{}",
            serde_json::json!({"event": "task_dropped", "job_id": self.0})
        );
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
        if self.state.borrow().jobs.len() >= 128 {
            return Err(Problem::new(
                StatusCode::TOO_MANY_REQUESTS,
                "job_limit",
                "this in-memory generation retains at most 128 jobs",
            ));
        }
        let ready = self.lifecycle.readiness().map_err(|_| unavailable())?;
        let cancellation = ready.cancellation();
        let id = self.state.borrow().jobs.len() + 1;
        let job = Job {
            id,
            message: request.message,
            status: "queued",
            error: None,
        };
        let state = self.state.clone();
        self.state.borrow_mut().jobs.push(job.clone());
        let lifetime = TaskLifetime(id);
        let spawned = self.lifecycle.spawn_local(async move {
            let _lifetime = lifetime;
            ready.wait().await;
            state.borrow_mut().jobs[id - 1].status = "running";
            tokio::select! {
                biased;
                () = cancellation.cancelled() => {
                    state.borrow_mut().jobs[id - 1].status = "cancelled";
                    println!("{}", serde_json::json!({"event": "job_cancelled", "job_id": id}));
                }
                () = tokio::time::sleep(Duration::from_millis(request.delay_ms)) => {
                    let mut state = state.borrow_mut();
                    if request.fail {
                        state.jobs[id - 1].status = "failed";
                        state.jobs[id - 1].error = Some("requested_failure");
                    } else {
                        state.jobs[id - 1].status = "completed";
                        let message = state.jobs[id - 1].message.clone();
                        state.notifications.push(Notification { job_id: id, message });
                    }
                    println!("{}", serde_json::json!({"event": "job_finished", "job_id": id, "status": state.jobs[id - 1].status}));
                }
            }
        });
        if spawned.is_err() {
            self.state.borrow_mut().jobs.pop();
            return Err(unavailable());
        }
        Ok((StatusCode::ACCEPTED, Json(job)))
    }

    #[get("background.jobs", "/jobs")]
    async fn jobs(&self) -> Result<Json<Vec<Job>>, Problem> {
        Ok(Json(self.state.borrow().jobs.clone()))
    }

    #[get("background.notifications", "/notifications")]
    async fn notifications(&self) -> Result<Json<Vec<Notification>>, Problem> {
        Ok(Json(self.state.borrow().notifications.clone()))
    }
}

fn unavailable() -> Problem {
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "generation_stopped",
        "the Plugin generation no longer accepts work",
    )
}
