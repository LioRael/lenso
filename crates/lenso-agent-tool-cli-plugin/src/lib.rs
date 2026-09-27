//! A Host-owned caller identity for explicit local Agent Tool operations.
//!
//! Selecting this Plugin grants no model access. The local Host invokes only
//! the exact Tool Provider bindings in its resolved App Plan after a user CLI
//! operation names one provider Instance and one operation.

use lenso::ManyPort;
use lenso_capability_agent_tool_provider as tools;

#[lenso::plugin(consumer)]
#[derive(Clone, Debug)]
struct ToolCli {
    tools: ManyPort<tools::ToolProviderClient>,
}

/// Retains the linked factory in a precompiled local Host.
pub fn link() {
    link_plugin();
}

/// Returns the generated Plugin Descriptor, never an independently authored copy.
pub fn descriptor() -> lenso_app_plan::authoring::PluginDescriptor {
    serde_json::from_str(PLUGIN_DESCRIPTOR_JSON).expect("generated Tool CLI descriptor")
}
