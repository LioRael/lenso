use lenso::host::events::{ClaimedOutboxEvent, EventHandler};
use lenso::host::http::{LinkedBinding, LinkedHttpContribution};
use lenso::host::runtime::{AppContext, FunctionDefinition, FunctionHandler, RuntimeDescriptor};
use lenso::host::{HostLinkedModule, Migration, ModuleManifest};

#[test]
fn lightweight_feature_exposes_linked_module_authoring_without_host_boot() {
    let _manifest: fn() -> ModuleManifest = || ModuleManifest::builder("example/module").build();
    let _linked: Option<HostLinkedModule> = None;
    let _migrations: &[Migration] = &[];
    let _binding = LinkedBinding::builder;
    let _http: Option<LinkedHttpContribution> = None;
    let _runtime: Option<RuntimeDescriptor> = None;
    let _function: Option<FunctionDefinition> = None;
    let _context: Option<AppContext> = None;
    let _event: Option<ClaimedOutboxEvent> = None;
    fn accepts_event_handler<T: EventHandler>() {}
    fn accepts_function_handler<T: FunctionHandler>() {}
    let _ = accepts_event_handler::<Never>;
    let _ = accepts_function_handler::<Never>;
}

#[derive(Debug)]
enum Never {}

#[async_trait::async_trait]
impl EventHandler for Never {
    fn handler_name(&self) -> &str {
        match *self {}
    }

    fn event_name(&self) -> &str {
        match *self {}
    }

    async fn handle(&self, _event: &ClaimedOutboxEvent) -> lenso::host::events::AppResult<()> {
        match *self {}
    }
}

#[async_trait::async_trait]
impl FunctionHandler for Never {
    async fn call(
        &self,
        _context: lenso::host::runtime::ExecutionContext,
        _input: serde_json::Value,
    ) -> lenso::host::runtime::AppResult<serde_json::Value> {
        match *self {}
    }
}
