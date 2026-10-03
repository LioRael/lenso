use lenso_capability_agent_tool_provider::{self as tools, ExecuteRequest};

#[lenso::plugin(id = "example.health", root_slot = "health", lifecycle)]
#[derive(Clone, Debug)]
pub struct Health {
    #[dependency(id = "tools")]
    tools: tools::ToolProviderClient,
}

impl lenso::Lifecycle for Health {
    async fn activate(
        &self,
        _context: lenso::ActivateContext,
    ) -> Result<(), lenso::RuntimeFailure> {
        let response = self
            .tools
            .execute(ExecuteRequest {
                name: "example.bun-b".into(),
                arguments_json: r#"{"text":"mixed host works"}"#.try_into().unwrap(),
            })
            .await
            .map_err(|error| lenso::RuntimeFailure::InvalidResolvedPlan {
                detail: format!("{error:?}"),
            })?;
        assert!(response.content.contains("unrelated-bun"));
        eprintln!("NATIVE_HEALTH_RESULT {}", response.content);
        Ok(())
    }
    async fn deactivate(
        &self,
        _context: lenso::DeactivateContext,
    ) -> Result<(), lenso::RuntimeFailure> {
        Ok(())
    }
}
