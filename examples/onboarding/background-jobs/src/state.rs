use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub(crate) id: usize,
    pub(crate) message: String,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(crate) attempts: u32,
    pub(crate) delay_ms: u64,
    pub(crate) fail: bool,
}

impl Job {
    pub(crate) fn terminal(&self) -> bool {
        matches!(self.status.as_str(), "completed" | "failed" | "cancelled")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Notification {
    pub(crate) job_id: usize,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct State {
    pub(crate) jobs: Vec<Job>,
    pub(crate) notifications: Vec<Notification>,
}

#[derive(Debug, Default)]
pub(crate) struct Store {
    pub(crate) state: State,
    path: Option<PathBuf>,
    pub(crate) faulted: bool,
}

impl Store {
    pub(crate) fn load(path: Option<PathBuf>) -> Result<Self, &'static str> {
        let state = if let Some(path) = &path {
            match fs::read(path) {
                Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "snapshot_invalid")?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => State::default(),
                Err(_) => return Err("snapshot_read_failed"),
            }
        } else {
            State::default()
        };
        validate(&state)?;
        Ok(Self {
            state,
            path,
            faulted: false,
        })
    }

    pub(crate) fn healthy(&self) -> Result<(), &'static str> {
        if self.faulted {
            Err("snapshot_write_failed")
        } else {
            Ok(())
        }
    }

    /// Publish the new job and notification together only after atomic storage.
    pub(crate) fn commit(&mut self, update: impl FnOnce(&mut State)) -> Result<(), &'static str> {
        self.healthy()?;
        let mut next = self.state.clone();
        update(&mut next);
        if let Some(path) = &self.path {
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                let parent = path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
                temporary.write_all(&serde_json::to_vec(&next)?)?;
                temporary.as_file().sync_all()?;
                temporary.persist(path)?;
                Ok(())
            })();
            if result.is_err() {
                self.faulted = true;
                return Err("snapshot_write_failed");
            }
        }
        self.state = next;
        Ok(())
    }

    pub(crate) fn interrupted(&mut self, id: usize) {
        if self.path.is_none() && !self.state.jobs[id - 1].terminal() {
            self.state.jobs[id - 1].status = "cancelled".into();
            self.state.jobs[id - 1].error = None;
            println!(
                "{}",
                serde_json::json!({"event": "job_cancelled", "job_id": id})
            );
        }
    }
}

fn validate(state: &State) -> Result<(), &'static str> {
    if state.jobs.len() > 128 || state.notifications.len() > state.jobs.len() {
        return Err("snapshot_invalid");
    }
    for (index, job) in state.jobs.iter().enumerate() {
        if job.id != index + 1
            || job.message.trim().is_empty()
            || job.message.len() > 140
            || !(20..=5000).contains(&job.delay_ms)
            || job.attempts > 3
            || !matches!(
                job.status.as_str(),
                "queued" | "running" | "retrying" | "completed" | "failed" | "cancelled"
            )
        {
            return Err("snapshot_invalid");
        }
        let notifications = state
            .notifications
            .iter()
            .filter(|n| n.job_id == job.id)
            .collect::<Vec<_>>();
        if notifications.len() != usize::from(job.status == "completed")
            || notifications.iter().any(|n| n.message != job.message)
        {
            return Err("snapshot_invalid");
        }
    }
    if state
        .notifications
        .iter()
        .any(|n| n.job_id == 0 || n.job_id > state.jobs.len())
    {
        return Err("snapshot_invalid");
    }
    Ok(())
}
