use lenso_capability_agent_tool_provider::{self as tools, ExecuteRequest};
#[lenso::plugin(id = "example.proof", root_slot = "proof", lifecycle)]
#[derive(Clone, Debug)]
pub struct Proof {
    #[dependency(id = "tools")]
    tools: tools::ToolProviderClient,
}
impl lenso::Lifecycle for Proof {
    async fn activate(
        &self,
        _context: lenso::ActivateContext,
    ) -> Result<(), lenso::RuntimeFailure> {
        let response = self
            .tools
            .execute(ExecuteRequest {
                name: "example.bun-a".into(),
                arguments_json: r#"{"text":"mixed host works"}"#.try_into().unwrap(),
            })
            .await
            .map_err(|e| lenso::RuntimeFailure::InvalidResolvedPlan {
                detail: format!("{e:?}"),
            })?;

        eprintln!("NATIVE_BUN_RESULT {}", response.content);
        Ok(())
    }
    async fn deactivate(
        &self,
        _context: lenso::DeactivateContext,
    ) -> Result<(), lenso::RuntimeFailure> {
        eprintln!("NATIVE_DEACTIVATED");
        Ok(())
    }
}
