use std::fmt::Debug;
use std::sync::Arc;
use uuid::Uuid;

pub trait IdGenerator: Debug + Send + Sync {
    fn new_id(&self, prefix: &str) -> String;
}

pub type DynIdGenerator = Arc<dyn IdGenerator>;

#[derive(Debug, Default)]
pub struct UuidGenerator;

impl IdGenerator for UuidGenerator {
    fn new_id(&self, prefix: &str) -> String {
        format!("{prefix}_{}", Uuid::now_v7())
    }
}
