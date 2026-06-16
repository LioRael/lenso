use chrono::{DateTime, Utc};
use std::fmt::Debug;
use std::sync::Arc;

pub trait Clock: Debug + Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub type DynClock = Arc<dyn Clock>;

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
