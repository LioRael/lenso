//! Additive native metadata and private input-diagnostic compatibility.

use lenso_app::{
    Action, ActionError, ActionMetadata, CancellationToken, Catalog, Context, ErrorCode, Executor,
    Module, describe_action,
};

struct Legacy;
struct Native(std::sync::Arc<std::sync::Mutex<usize>>);
impl Action<()> for Legacy {
    type Input = Native;
    type Output = Native;
    fn id(&self) -> &str {
        "legacy"
    }
    async fn authorize(&self, _: &Context<()>, _: &Native) -> Result<(), ActionError> {
        Ok(())
    }
    async fn run(&self, _: &Context<()>, input: Native) -> Result<Native, ActionError> {
        Ok(input)
    }
}

fn context() -> Context<()> {
    Context {
        call_id: "metadata-test".into(),
        principal: None,
        resources: (),
        cancellation: CancellationToken::new(),
    }
}

#[tokio::test]
async fn legacy_actions_keep_non_serializable_values_and_unknown_descriptions() {
    let pointer = std::sync::Arc::new(std::sync::Mutex::new(7));
    let value = Executor::new(|_| {})
        .call(&Legacy, &context(), Native(pointer.clone()))
        .await
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&value.0, &pointer));
    assert_eq!(describe_action(&Legacy).metadata, None);
    let mut module = Module::new("legacy");
    module.action(&Legacy);
    let mut catalog = Catalog::new();
    catalog.register(module).unwrap();
    assert_eq!(catalog.describe_actions(), vec![describe_action(&Legacy)]);
    #[cfg(feature = "json")]
    assert_eq!(
        serde_json::to_value(&catalog.describe()[0]).unwrap(),
        serde_json::json!({
            "id":"legacy","actions":["legacy"],"tasks":[],"events":[],"resources":[]
        })
    );
}

struct Denied;
impl Action<()> for Denied {
    type Input = Native;
    type Output = Native;
    fn id(&self) -> &str {
        "denied"
    }
    fn metadata(&self) -> Option<ActionMetadata> {
        Some(ActionMetadata {
            capabilities: Some(vec![]),
            effects: Some(vec![]),
            ..Default::default()
        })
    }
    async fn authorize(&self, _: &Context<()>, _: &Native) -> Result<(), ActionError> {
        Err(ActionError::new(ErrorCode::Forbidden, "Still denied"))
    }
    async fn run(&self, _: &Context<()>, _: Native) -> Result<Native, ActionError> {
        panic!("Access cannot be bypassed")
    }
}

#[tokio::test]
async fn native_annotations_never_grant_access_even_without_json() {
    let error = Executor::new(|_| {})
        .call(&Denied, &context(), Native(Default::default()))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code(), ErrorCode::Forbidden);
    assert!(error.issues().is_empty());
}

#[cfg(feature = "json")]
mod external {
    use super::*;
    use lenso_app::{ConsoleExtension, InputIssue, InputIssueCode, InputSegment};
    use serde::Deserialize;
    use serde_json::{Value, json};

    #[derive(Deserialize)]
    struct Profile {
        age: u64,
    }
    #[derive(Deserialize)]
    struct Row {
        id: String,
    }
    #[derive(Deserialize, Default)]
    struct Input {
        profile: Option<Profile>,
        rows: Option<Vec<Row>>,
        labels: Option<std::collections::BTreeMap<String, u64>>,
    }
    struct Parse;
    impl Action<()> for Parse {
        type Input = Input;
        type Output = ();
        fn id(&self) -> &str {
            "parse"
        }
        fn metadata(&self) -> Option<ActionMetadata> {
            let fixture: Value = serde_json::from_str(include_str!(
                "../../../../conformance/fixtures/input-issues.json"
            ))
            .unwrap();
            Some(ActionMetadata {
                input_schema: Some(fixture["schema"].clone()),
                ..Default::default()
            })
        }
        async fn authorize(&self, _: &Context<()>, _: &Input) -> Result<(), ActionError> {
            panic!("invalid input must precede Access")
        }
        async fn run(&self, _: &Context<()>, input: Input) -> Result<(), ActionError> {
            // Read the native fields to keep the fixture a real Deserialize consumer.
            let _ = input.profile.map(|p| p.age);
            let _ = input
                .rows
                .map(|r| r.into_iter().map(|r| r.id).collect::<Vec<_>>());
            let _ = input.labels;
            panic!("invalid input must not run")
        }
    }

    #[tokio::test]
    async fn shared_nested_array_and_dynamic_key_projections_do_not_leak_values() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../conformance/fixtures/input-issues.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let error = Executor::new(|record: lenso_app::CallRecord| {
                assert_eq!(record.outcome, Err(ErrorCode::InvalidInput))
            })
            .call_json(&Parse, &context(), case["input"].clone())
            .await
            .unwrap_err();
            let public = serde_json::to_value(error.public()).unwrap();
            assert_eq!(
                public["issues"],
                json!([{"path":case["path"],"code":"invalid_value"}])
            );
            assert!(!public.to_string().contains("private-parser-value"));
            assert!(!public.to_string().contains("private-map-key"));
            assert!(!public.to_string().contains("source"));
            assert!(std::error::Error::source(&error).is_some());
        }
    }

    #[test]
    fn public_errors_keep_legacy_shape_and_ignore_issues_on_non_input_errors() {
        let error = ActionError::new(ErrorCode::Forbidden, "Denied")
            .with_issues(vec![InputIssue {
                path: vec![InputSegment::Field("private".into())],
                code: InputIssueCode::Required,
            }])
            .with_source(std::io::Error::other("private-cause"));
        assert_eq!(
            serde_json::to_value(error.public()).unwrap(),
            json!({"code":"forbidden","message":"Denied"})
        );
    }

    #[test]
    fn erased_console_bindings_keep_the_actual_action_description_and_owned_snapshots() {
        let mut extension = ConsoleExtension::new();
        extension.page("parse", "Parse", Parse).unwrap();
        let module = extension.module("example");
        let expected = module.describe_actions();
        let mut catalog = Catalog::new();
        catalog.register(module).unwrap();
        let mut manifest = extension.manifest(&catalog).unwrap();
        assert_eq!(manifest.modules[0].describe_actions(), expected);
        let json = serde_json::to_value(&manifest).unwrap();
        let wire = serde_json::to_value(&expected).unwrap();
        assert_eq!(
            serde_json::from_value::<Vec<lenso_app::ActionDescription>>(wire).unwrap(),
            expected
        );
        assert_eq!(
            json["modules"][0]["actionMetadata"]["parse"],
            serde_json::to_value(Parse.metadata().unwrap()).unwrap()
        );
        manifest.modules.clear();
        assert_eq!(catalog.describe_actions(), expected);
    }
}
