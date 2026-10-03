use lenso_capability_agent_tool_provider::{self as tools, ExecuteRequest};

#[lenso::plugin(id = "example.proof", root_slot = "proof", consumer)]
#[derive(Clone, Debug)]
pub struct Proof {
    #[dependency(id = "tools")]
    tools: tools::ToolProviderClient,
}

#[lenso::plugin_impl]
impl Proof {
    #[create]
    async fn create(tools: tools::ToolProviderClient) -> Result<Self, String> {
        let response = tools
            .execute(ExecuteRequest {
                name: "example.bun-a".into(),
                arguments_json: r#"{"text":"mixed host works"}"#.try_into().unwrap(),
            })
            .await
            .map_err(|error| format!("{error:?}"))?;
        eprintln!("NATIVE_BUN_RESULT {}", response.content);
        Ok(Self { tools })
    }

    #[stop]
    fn stop(&self) {
        let _ = &self.tools;
        eprintln!("NATIVE_PROOF_STOPPED");
    }
}
