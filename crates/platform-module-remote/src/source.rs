use crate::config::RemoteModuleConfig;

#[derive(Debug, Clone)]
pub struct RemoteModuleSource {
    pub config: RemoteModuleConfig,
}

impl RemoteModuleSource {
    #[must_use]
    pub fn new(config: RemoteModuleConfig) -> Self {
        Self { config }
    }
}
