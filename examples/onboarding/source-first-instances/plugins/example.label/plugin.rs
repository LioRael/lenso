use crate::metadata;
use lenso_kernel::InvocationContext;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, lenso::PluginConfig)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[lenso(default = "label")]
    pub label: String,
}

#[lenso::plugin(id = "example.label", root_slot = "labels")]
#[derive(Clone, Debug)]
pub struct Plugin {
    #[config]
    config: Config,
    calls: Rc<Cell<u32>>,
}

#[lenso::provides(metadata::Metadata)]
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
impl Plugin {
    async fn normalize(
        &self,
        _context: InvocationContext,
        request: metadata::NormalizeRequest,
    ) -> Result<metadata::NormalizeResponse, metadata::NormalizeError> {
        let calls = self.calls.get() + 1;
        self.calls.set(calls);
        Ok(metadata::NormalizeResponse {
            name: format!("{}:{calls}:{}", self.config.label, request.name),
            extension: "none".into(),
            size_bytes: request.size_bytes,
        })
    }
}
