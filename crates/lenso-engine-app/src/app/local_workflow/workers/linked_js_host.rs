//! Request-only Workers JS projection. Kernel owns binding and lifecycle order.
use lenso_app_plan::{ExecutionClassId, PluginInstancePlan, ResolvedAppPlan};
use lenso_kernel::{
    ActivateContext, DeactivateContext, ExecutionAdapter, InvocationContext, PluginLifecycle,
    PreparedNativeApp, PreparedNativePlugin, RuntimeFailure,
};
use lenso_runtime_codec::{
    JsonCapabilityCodec, JsonHostImports, JsonInvocationOutcome, JsonRequestTransport,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, future_to_promise};

const CLASS: &str = "lenso.workers-js@1";
const PROFILE: &str = "lenso.workers-js-authoring@2";
const DEFAULT_BUDGET_MS: f64 = 30_000.0;

#[wasm_bindgen(raw_module = "./plugin-host.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = constructPlugin)]
    fn construct_plugin(
        scope: &JsValue,
        options: &str,
        cancelled: &JsValue,
        invoke: &JsValue,
        budget: f64,
    ) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = invokePlugin)]
    fn invoke_plugin(
        scope: &JsValue,
        plugin: &JsValue,
        capability: &str,
        operation: &str,
        payload: &str,
        cancelled: &JsValue,
        budget: f64,
        request_id: &str,
    ) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch, js_name = stopPlugin)]
    fn stop_plugin(
        scope: &JsValue,
        plugin: &JsValue,
        cancelled: &JsValue,
        budget: f64,
    ) -> Result<js_sys::Promise, JsValue>;
}

fn failure(error: impl std::fmt::Debug) -> RuntimeFailure {
    RuntimeFailure::PluginFailure {
        detail: format!("Workers JS: {error:?}"),
    }
}

#[derive(Debug)]
pub struct WorkersJsAdapter {
    scope: JsValue,
    codecs: BTreeMap<String, Rc<dyn JsonCapabilityCodec>>,
}
impl WorkersJsAdapter {
    pub fn new(scope: JsValue) -> Self {
        let mut codecs: BTreeMap<String, Rc<dyn JsonCapabilityCodec>> = BTreeMap::new();
        // LENSO_WORKERS_JS_CODECS
        Self { scope, codecs }
    }
}
impl ExecutionAdapter for WorkersJsAdapter {
    fn execution_class(&self) -> ExecutionClassId {
        ExecutionClassId::new(CLASS)
    }
    fn supports_runtime_profile(&self, version: u32, profile: &str) -> bool {
        version == 2 && profile == PROFILE
    }
    fn prepare(&self, plan: &ResolvedAppPlan) -> Result<PreparedNativeApp, RuntimeFailure> {
        let mut generations = BTreeMap::new();
        for instance in plan
            .plugin_instances()
            .iter()
            .filter(|i| i.execution_class().as_str() == CLASS)
        {
            let codecs = lenso_runtime_codec::codecs_for_instance(instance, &self.codecs)?;
            let imports = JsonHostImports::new(
                lenso_runtime_codec::codecs_for_requirements(instance, &self.codecs)?,
                0,
            )?;
            let generation = Rc::new(Generation {
                scope: self.scope.clone(),
                instance: instance.clone(),
                codecs,
                imports: Rc::new(imports),
                contexts: Rc::new(RefCell::new(BTreeMap::new())),
                plugin: RefCell::new(None),
                callback: RefCell::new(None),
            });
            let endpoints = lenso_runtime_codec::json_request_endpoints(
                generation.clone(),
                generation.codecs.clone(),
            );
            generations.insert(
                instance.instance_key().to_owned(),
                PreparedNativePlugin::with_endpoints(endpoints, Vec::new(), Lifecycle(generation)),
            );
        }
        lenso_runtime_codec::prepare_request_app(plan, &self.execution_class(), generations)
    }
}

type ImportCallback = Closure<dyn FnMut(u32, String, String, JsValue) -> js_sys::Promise>;
#[derive(Debug)]
struct Generation {
    scope: JsValue,
    instance: PluginInstancePlan,
    codecs: Vec<Rc<dyn JsonCapabilityCodec>>,
    imports: Rc<JsonHostImports>,
    contexts: Rc<RefCell<BTreeMap<String, InvocationContext>>>,
    plugin: RefCell<Option<JsValue>>,
    callback: RefCell<Option<ImportCallback>>,
}

struct CallGuard {
    contexts: Rc<RefCell<BTreeMap<String, InvocationContext>>>,
    key: String,
    cancelled: Closure<dyn FnMut() -> bool>,
}
impl CallGuard {
    fn new(generation: &Generation, key: String, context: InvocationContext) -> Self {
        let token = context.cancellation();
        generation
            .contexts
            .borrow_mut()
            .insert(key.clone(), context);
        Self {
            contexts: generation.contexts.clone(),
            key,
            cancelled: Closure::new(move || token.is_cancelled()),
        }
    }
}
impl Drop for CallGuard {
    fn drop(&mut self) {
        self.contexts.borrow_mut().remove(&self.key);
    }
}

impl Generation {
    fn callback(&self) -> ImportCallback {
        let contexts = self.contexts.clone();
        let imports = self.imports.clone();
        Closure::new(move |binding, operation, payload: String, call: JsValue| {
            let context = js_sys::Reflect::get(&call, &JsValue::from_str("requestId"))
                .ok()
                .and_then(|v| v.as_string())
                .and_then(|key| contexts.borrow().get(&key).cloned());
            let imports = imports.clone();
            future_to_promise(async move {
                let context =
                    context.ok_or_else(|| JsValue::from_str("Kernel invocation scope closed"))?;
                let payload = serde_json::from_str(&payload).map_err(super::error)?;
                let result = lenso_runtime_codec::json_host_invocation_envelope(
                    imports.invoke(binding, operation, payload, context).await,
                );
                Ok(JsValue::from_str(
                    &serde_json::to_string(&result).map_err(super::error)?,
                ))
            })
        })
    }
}

impl JsonRequestTransport for Generation {
    fn invoke(
        self: Rc<Self>,
        capability: String,
        operation: String,
        request_json: String,
        context: InvocationContext,
    ) -> futures::future::LocalBoxFuture<'static, Result<JsonInvocationOutcome, RuntimeFailure>>
    {
        Box::pin(async move {
            let budget = context
                .remaining_budget()
                .map_or(DEFAULT_BUDGET_MS, |d| d.as_secs_f64() * 1000.0);
            let key = context.request_id().to_string();
            let call = CallGuard::new(&self, key.clone(), context.clone());
            let plugin = self
                .plugin
                .borrow()
                .clone()
                .ok_or(RuntimeFailure::AdmissionClosed)?;
            let promise = invoke_plugin(
                &self.scope,
                &plugin,
                &capability,
                &operation,
                &request_json,
                call.cancelled.as_ref(),
                budget,
                &key,
            )
            .map_err(failure)?;
            let value = JsFuture::from(promise).await.map_err(failure)?;
            let outcome: serde_json::Value = serde_json::from_str(
                &value
                    .as_string()
                    .ok_or_else(|| failure("invalid JS result"))?,
            )
            .map_err(failure)?;
            match outcome["kind"].as_str() {
                Some("success") => Ok(JsonInvocationOutcome::Success(outcome["value"].clone())),
                Some("domain") => Ok(JsonInvocationOutcome::DomainError(outcome["value"].clone())),
                Some("runtime") => Err(match outcome["failure"]["kind"].as_str() {
                    Some("cancelled") => RuntimeFailure::Cancelled {
                        request_id: context.request_id(),
                    },
                    Some("deadline_exceeded") => RuntimeFailure::DeadlineExceeded {
                        request_id: context.request_id(),
                    },
                    Some("admission_closed") => RuntimeFailure::AdmissionClosed,
                    _ => failure(outcome["failure"].clone()),
                }),
                _ => Err(failure("invalid JS outcome envelope")),
            }
        })
    }
}

#[derive(Debug)]
struct Lifecycle(Rc<Generation>);
impl PluginLifecycle for Lifecycle {
    fn construct(&self, context: ActivateContext) -> lenso_kernel::PluginFuture {
        let generation = self.0.clone();
        Box::pin(async move {
            generation.imports.activate(context.dependencies())?;
            let invocation = context
                .dependencies()
                .invocation_context(None, context.cancellation())?;
            let call = CallGuard::new(&generation, "construct".to_owned(), invocation);
            let callback = generation.callback();
            let endpoints = generation.codecs.iter().map(|codec| serde_json::json!({
                "capability_id": codec.capability_id(), "descriptor_version": codec.descriptor_version(),
                "descriptor_digest": codec.descriptor_digest(), "operations": codec.request_operations(),
                "stream_operations": [], "event_operations": [],
            })).collect::<Vec<_>>();
            let bindings = generation
                .imports
                .descriptors()?
                .into_iter()
                .map(|binding| {
                    // The legacy guest envelope omits digests. This v2 projection
                    // carries the existing exact codec evidence explicitly.
                    let mut value = serde_json::to_value(&binding).map_err(failure)?;
                    value["descriptor_digest"] =
                        serde_json::Value::String(binding.descriptor_digest);
                    Ok(value)
                })
                .collect::<Result<Vec<_>, RuntimeFailure>>()?;
            let options = serde_json::json!({ "plugin": generation.instance.package_id(), "endpoints": endpoints,
                "configuration": serde_json::from_str::<serde_json::Value>(generation.instance.configuration()).map_err(failure)?,
                "requirements": generation.instance.required_capabilities().iter().map(|r| r.requirement_id()).collect::<Vec<_>>(),
                "bindings": bindings, });
            let promise = construct_plugin(
                &generation.scope,
                &options.to_string(),
                call.cancelled.as_ref(),
                callback.as_ref(),
                DEFAULT_BUDGET_MS,
            )
            .map_err(failure)?;
            // Keep the bridge alive even when create fails and rollback is required.
            generation.callback.replace(Some(callback));
            let plugin = JsFuture::from(promise).await.map_err(failure)?;
            generation.plugin.replace(Some(plugin));
            Ok(())
        })
    }
    fn deactivate(&self, context: DeactivateContext) -> lenso_kernel::PluginFuture {
        let generation = self.0.clone();
        Box::pin(async move {
            let plugin = generation.plugin.borrow().clone();
            if let Some(plugin) = plugin {
                let budget = context
                    .remaining_budget()
                    .unwrap_or(Duration::from_millis(250))
                    .as_secs_f64()
                    * 1000.0;
                let invocation = context.dependency_invocation_context()?;
                let call = CallGuard::new(&generation, "stop".to_owned(), invocation);
                let promise =
                    stop_plugin(&generation.scope, &plugin, call.cancelled.as_ref(), budget)
                        .map_err(failure)?;
                JsFuture::from(promise).await.map_err(failure)?;
                generation.plugin.take();
            }
            generation.imports.deactivate();
            Ok(())
        })
    }
}
