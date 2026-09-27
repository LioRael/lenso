//! Explicit local CLI ingress for Plan-bound Agent Tool providers.

use anyhow::{Context, bail};
use lenso_capability_agent_tool_provider::{
    self as tools, CatalogError, CatalogRequest, CatalogResponse, ExecuteError, ExecuteRequest,
    ExecuteResponse, ToolProviderClient,
};
use lenso_kernel::{NativeApp, RuntimeFailure};
use lenso_plugin_authoring::CapabilityClientMany;
use lenso_runtime_codec::JsonCapabilityCodec;
use serde_json::Value;
use std::any::Any;

const CALLER: &str = "lenso.agent.tool-cli/default";
const MAX_RESULT_BYTES: usize = 1024 * 1024;

/// Typed projection of the released contract onto this Host's runtime-codec.
#[derive(Clone, Debug)]
pub struct ToolProviderCodec;

impl JsonCapabilityCodec for ToolProviderCodec {
    fn capability_id(&self) -> &'static str {
        tools::CAPABILITY_ID
    }
    fn descriptor_version(&self) -> &'static str {
        tools::DESCRIPTOR_VERSION
    }
    fn descriptor_digest(&self) -> &'static str {
        tools::DESCRIPTOR_DIGEST
    }
    fn request_operations(&self) -> &'static [&'static str] {
        &[tools::CATALOG_OPERATION, tools::EXECUTE_OPERATION]
    }

    fn encode_request(&self, operation: &str, request: &dyn Any) -> Result<Value, RuntimeFailure> {
        match operation {
            tools::CATALOG_OPERATION => serde_json::to_value(
                request
                    .downcast_ref::<CatalogRequest>()
                    .ok_or_else(protocol_failure)?,
            )
            .map_err(|_| protocol_failure()),
            tools::EXECUTE_OPERATION => serde_json::to_value(
                request
                    .downcast_ref::<ExecuteRequest>()
                    .ok_or_else(protocol_failure)?,
            )
            .map_err(|_| protocol_failure()),
            _ => Err(unknown_operation(operation)),
        }
    }

    fn decode_response(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        match operation {
            tools::CATALOG_OPERATION => serde_json::from_value::<CatalogResponse>(value)
                .map(|value| Box::new(value) as Box<dyn Any>)
                .map_err(|_| protocol_failure()),
            tools::EXECUTE_OPERATION => serde_json::from_value::<ExecuteResponse>(value)
                .map(|value| Box::new(value) as Box<dyn Any>)
                .map_err(|_| protocol_failure()),
            _ => Err(unknown_operation(operation)),
        }
    }

    fn decode_domain_error(
        &self,
        operation: &str,
        value: Value,
    ) -> Result<Box<dyn Any>, RuntimeFailure> {
        match operation {
            tools::CATALOG_OPERATION => serde_json::from_value::<CatalogError>(value)
                .map(|value| Box::new(value) as Box<dyn Any>)
                .map_err(|_| protocol_failure()),
            tools::EXECUTE_OPERATION => serde_json::from_value::<ExecuteError>(value)
                .map(|value| Box::new(value) as Box<dyn Any>)
                .map_err(|_| protocol_failure()),
            _ => Err(unknown_operation(operation)),
        }
    }
}

fn protocol_failure() -> RuntimeFailure {
    RuntimeFailure::ProtocolViolation {
        capability: tools::CAPABILITY_ID,
    }
}

fn unknown_operation(operation: &str) -> RuntimeFailure {
    RuntimeFailure::UnknownOperation {
        capability: tools::CAPABILITY_ID,
        operation: operation.to_owned(),
    }
}

pub fn preflight(plan: &lenso_app_plan::ResolvedAppPlan, args: &[String]) -> anyhow::Result<()> {
    let [operation, provider, rest @ ..] = args else {
        bail!("usage: tools catalog PROVIDER | tools execute PROVIDER NAME ARGUMENTS_JSON");
    };
    match (operation.as_str(), rest) {
        ("catalog", []) => {}
        ("execute", [_, arguments_json]) => {
            let _: tools::RawJson = arguments_json
                .as_str()
                .try_into()
                .context("Tool arguments must be portable JSON")?;
        }
        _ => bail!("usage: tools catalog PROVIDER | tools execute PROVIDER NAME ARGUMENTS_JSON"),
    }
    let caller = plan
        .plugin_instance(CALLER)
        .context("Agent Tool CLI is not selected by this Host")?;
    if caller.package_id() != "lenso.agent.tool-cli" {
        bail!("Agent Tool CLI caller identity is invalid");
    }
    let mut bindings = plan.capability_bindings().iter().filter(|binding| {
        binding.consumer_instance() == CALLER
            && binding.capability_id() == tools::CAPABILITY_ID
            && binding.descriptor_version() == tools::DESCRIPTOR_VERSION
            && binding.provider_instance() == provider
    });
    if bindings.next().is_none() {
        bail!("Tool Provider `{provider}` is not bound to the Host CLI");
    }
    if bindings.next().is_some() {
        bail!("Tool Provider `{provider}` has duplicate Host CLI bindings");
    }
    Ok(())
}

pub async fn run(app: &NativeApp, args: &[String]) -> anyhow::Result<()> {
    let [operation, provider, rest @ ..] = args else {
        bail!("usage: tools catalog PROVIDER | tools execute PROVIDER NAME ARGUMENTS_JSON");
    };
    let dependencies = app
        .dependencies(CALLER)
        .map_err(|error| anyhow::anyhow!("Agent Tool CLI is not selected: {error:?}"))?;
    let clients = ToolProviderClient::many_from_dependencies(&dependencies)
        .map_err(|error| anyhow::anyhow!("Agent Tool binding is invalid: {error:?}"))?;
    let mut selected = clients
        .iter()
        .filter(|client| client.provider_instance() == provider);
    let client = selected
        .next()
        .with_context(|| format!("Tool Provider `{provider}` is not bound to the Host CLI"))?;
    if selected.next().is_some() {
        bail!("Tool Provider `{provider}` has duplicate Host CLI bindings");
    }
    let output = match (operation.as_str(), rest) {
        ("catalog", []) => serde_json::to_vec(&serde_json::json!({
            "provider_instance": provider,
            "catalog": client
                .catalog(CatalogRequest {})
                .await
                .map_err(|error| anyhow::anyhow!("Tool Provider catalog failed: {error:?}"))?,
        }))?,
        ("execute", [name, arguments_json]) => {
            let arguments_json = arguments_json
                .as_str()
                .try_into()
                .context("Tool arguments must be portable JSON")?;
            serde_json::to_vec(&serde_json::json!({
                "provider_instance": provider,
                "result": client
                    .execute(ExecuteRequest { name: name.clone(), arguments_json })
                    .await
                    .map_err(|error| anyhow::anyhow!("Tool Provider execution failed: {error:?}"))?,
            }))?
        }
        _ => bail!("usage: tools catalog PROVIDER | tools execute PROVIDER NAME ARGUMENTS_JSON"),
    };
    if output.len() > MAX_RESULT_BYTES {
        bail!("Tool Provider result exceeds local CLI output limit");
    }
    println!("{}", String::from_utf8(output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lenso_app_plan::{
        AppComposition, CapabilityBinding, CapabilityEndpointPlan, CapabilityRequirementPlan,
        PluginInstancePlan,
    };

    fn plan() -> lenso_app_plan::ResolvedAppPlan {
        let caller = PluginInstancePlan::new(CALLER, "lenso.agent.tool-cli").with_requirement(
            CapabilityRequirementPlan::many(tools::CAPABILITY_ID, tools::DESCRIPTOR_VERSION),
        );
        let provider = PluginInstancePlan::new("example.tools/default", "example.tools")
            .with_capability(CapabilityEndpointPlan::new(
                tools::CAPABILITY_ID,
                tools::DESCRIPTOR_VERSION,
                [tools::CATALOG_OPERATION, tools::EXECUTE_OPERATION],
            ));
        AppComposition::new(
            vec![caller, provider],
            vec![CapabilityBinding::new(
                CALLER,
                tools::CAPABILITY_ID,
                tools::DESCRIPTOR_VERSION,
                "example.tools/default",
            )],
        )
        .resolve()
        .unwrap()
    }

    #[test]
    fn exact_binding_preflight_rejects_unbound_provider() {
        let plan = plan();
        assert!(preflight(&plan, &["catalog".into(), "example.tools/default".into()]).is_ok());
        assert!(preflight(&plan, &["catalog".into(), "example.other/default".into()]).is_err());
        assert!(
            preflight(
                &plan,
                &[
                    "execute".into(),
                    "example.tools/default".into(),
                    "uppercase".into(),
                    "not json".into()
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn codec_preserves_exact_tool_contract() {
        let codec = ToolProviderCodec;
        assert_eq!(codec.descriptor_digest(), tools::DESCRIPTOR_DIGEST);
        assert_eq!(codec.request_operations(), &["catalog", "execute"]);
        assert_eq!(
            codec.encode_request("catalog", &CatalogRequest {}).unwrap(),
            serde_json::json!({})
        );
        assert!(codec.encode_request("execute", &CatalogRequest {}).is_err());
    }
}
