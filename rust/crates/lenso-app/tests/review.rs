//! Regression checks for specification gaps found in the SDK review.

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use lenso_app::{
    ActionError, ErrorCode, Event, Resource, ResourceError, all_of, any_of, connect, with_resource,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyFixture {
    mode: String,
    first: String,
    second: String,
    expected: String,
    second_runs: usize,
}

fn policy(code: &str) -> Result<(), ActionError> {
    let code = match code {
        "ok" => return Ok(()),
        "unauthenticated" => ErrorCode::Unauthenticated,
        "forbidden" => ErrorCode::Forbidden,
        _ => ErrorCode::Internal,
    };
    Err(ActionError::new(code, code.as_str()))
}

#[test]
fn shared_policy_semantics() {
    let fixtures: Vec<PolicyFixture> =
        serde_json::from_str(include_str!("../../../../conformance/fixtures/policy.json")).unwrap();
    for fixture in fixtures {
        let mut second_runs = 0;
        let first = || policy(&fixture.first);
        let second = || {
            second_runs += 1;
            policy(&fixture.second)
        };
        let result = if fixture.mode == "all" {
            all_of(first, second)
        } else {
            any_of(first, second)
        };
        let actual = result
            .as_ref()
            .map(|_| "ok")
            .unwrap_or_else(|error| error.code().as_str());
        assert_eq!(
            actual, fixture.expected,
            "{}: {}, {}",
            fixture.mode, fixture.first, fixture.second
        );
        assert_eq!(second_runs, fixture.second_runs);
    }
}

#[tokio::test]
async fn shared_async_policy_semantics() {
    let fixtures: Vec<PolicyFixture> =
        serde_json::from_str(include_str!("../../../../conformance/fixtures/policy.json")).unwrap();
    for fixture in fixtures {
        let second_runs = AtomicUsize::new(0);
        let first = || async {
            tokio::task::yield_now().await;
            policy(&fixture.first)
        };
        let second = || {
            second_runs.fetch_add(1, Ordering::SeqCst);
            async { policy(&fixture.second) }
        };
        let result = if fixture.mode == "all" {
            lenso_app::all_of_async(first, second).await
        } else {
            lenso_app::any_of_async(first, second).await
        };
        assert_eq!(
            result
                .as_ref()
                .map(|_| "ok")
                .unwrap_or_else(|error| error.code().as_str()),
            fixture.expected
        );
        assert_eq!(second_runs.load(Ordering::SeqCst), fixture.second_runs);
    }
}

struct NativeResource {
    seen: Arc<Mutex<Vec<String>>>,
    close_fails: bool,
}
impl Resource for NativeResource {
    type Config = u32;
    type Value = BTreeMap<String, u32>;
    fn id(&self) -> &str {
        "native"
    }
    fn validate(&self, config: &u32) -> Result<(), ActionError> {
        if *config == 0 {
            return Err(ActionError::new(
                ErrorCode::InvalidInput,
                "Configuration must be positive",
            ));
        }
        Ok(())
    }
    async fn open(&self, config: u32) -> Result<Self::Value, ActionError> {
        self.seen.lock().unwrap().push(format!("open:{config}"));
        Ok(BTreeMap::from([("native".into(), config)]))
    }
    async fn close(&self, value: Self::Value) -> Result<(), ActionError> {
        self.seen
            .lock()
            .unwrap()
            .push(format!("close:{}", value["native"]));
        if self.close_fails {
            return Err(ActionError::new(ErrorCode::Internal, "Close failed"));
        }
        Ok(())
    }
}

#[tokio::test]
async fn resource_configuration_and_native_value_are_preserved() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let resource = NativeResource {
        seen: seen.clone(),
        close_fails: false,
    };
    assert_eq!(
        connect(&resource, 0).await.unwrap_err().code(),
        ErrorCode::InvalidInput
    );
    assert!(seen.lock().unwrap().is_empty());
    let result = with_resource(&resource, 7, |native| {
        Box::pin(async move { Ok(native["native"]) })
    })
    .await
    .unwrap();
    assert_eq!(result, 7);
    assert_eq!(*seen.lock().unwrap(), ["open:7", "close:7"]);
}

#[tokio::test]
async fn operation_and_asynchronous_cleanup_errors_both_survive() {
    let resource = NativeResource {
        seen: Arc::new(Mutex::new(Vec::new())),
        close_fails: true,
    };
    let result = with_resource(&resource, 7, |_| {
        Box::pin(async { Err::<(), _>(ActionError::new(ErrorCode::Conflict, "Operation failed")) })
    })
    .await;
    match result.unwrap_err() {
        ResourceError::UseAndClose { operation, cleanup } => {
            assert_eq!(operation.code(), ErrorCode::Conflict);
            assert_eq!(cleanup.message(), "Close failed");
        }
        error => panic!("both failures must survive: {error:?}"),
    }
}

#[tokio::test]
async fn repeated_local_handler_subscriptions_are_independent() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let handler = move |_: ()| {
        observed.fetch_add(1, Ordering::SeqCst);
        async { Ok(()) }
    };
    let mut event = Event::new("same.listener");
    let first = event.subscribe(handler.clone());
    event.subscribe(handler);
    event.publish(()).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    event.unsubscribe(first);
    event.publish(()).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[cfg(feature = "json")]
mod console {
    use super::*;
    use lenso_app::{
        Action, CancellationToken, Catalog, ConsoleExtension, Context, Executor, Principal,
        require_scope,
    };

    struct List;
    struct Detail;
    struct Update;

    impl Action<()> for List {
        type Input = ();
        type Output = Vec<u32>;
        fn id(&self) -> &str {
            "review.list"
        }
        async fn authorize(&self, context: &Context<()>, _: &()) -> Result<(), ActionError> {
            require_scope(context.principal.as_ref(), "read")
        }
        async fn run(&self, _: &Context<()>, _: ()) -> Result<Vec<u32>, ActionError> {
            Ok(vec![1])
        }
    }
    impl Action<()> for Detail {
        type Input = u32;
        type Output = u32;
        fn id(&self) -> &str {
            "review.detail"
        }
        async fn authorize(&self, context: &Context<()>, _: &u32) -> Result<(), ActionError> {
            require_scope(context.principal.as_ref(), "read")
        }
        async fn run(&self, _: &Context<()>, value: u32) -> Result<u32, ActionError> {
            Ok(value)
        }
    }
    impl Action<()> for Update {
        type Input = u32;
        type Output = u32;
        fn id(&self) -> &str {
            "review.update"
        }
        async fn authorize(&self, context: &Context<()>, _: &u32) -> Result<(), ActionError> {
            require_scope(context.principal.as_ref(), "read")
        }
        async fn run(&self, _: &Context<()>, value: u32) -> Result<u32, ActionError> {
            Ok(value + 1)
        }
    }

    #[tokio::test]
    async fn page_bindings_share_descriptions_and_authorized_execution() {
        let records = Mutex::new(Vec::new());
        let executor = Executor::new(|record| records.lock().unwrap().push(record));
        let mut extension = ConsoleExtension::new();
        extension
            .page("review", "Review", List)
            .unwrap()
            .detail("review", Detail)
            .unwrap()
            .operation("review", Update)
            .unwrap();
        let mut catalog = Catalog::new();
        catalog.register(extension.module("review")).unwrap();
        assert_eq!(
            extension.manifest(&catalog).unwrap().pages[0]
                .detail
                .as_deref(),
            Some("review.detail")
        );
        let mut context = Context {
            call_id: "review".into(),
            principal: Some(Principal {
                subject: "alice".into(),
                scopes: vec!["read".into()],
            }),
            resources: (),
            cancellation: CancellationToken::new(),
        };
        assert_eq!(
            extension
                .call(&executor, &context, "review.update", serde_json::json!(7))
                .await
                .unwrap(),
            8
        );
        assert_eq!(
            extension
                .call(&executor, &context, "review.detail", serde_json::json!(7))
                .await
                .unwrap(),
            7
        );
        context.principal = None;
        assert_eq!(
            extension
                .call(&executor, &context, "review.update", serde_json::json!(7))
                .await
                .unwrap_err()
                .code(),
            ErrorCode::Unauthenticated
        );
        assert_eq!(
            extension
                .call(
                    &executor,
                    &context,
                    "review.update",
                    serde_json::json!("bad")
                )
                .await
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(records.lock().unwrap().len(), 4);
    }
    #[tokio::test]
    async fn independent_console_extensions_merge_atomically() {
        let mut installed = ConsoleExtension::new();
        installed.page("base", "Base", List).unwrap();
        let mut conflicting = ConsoleExtension::new();
        conflicting.page("new", "New", Detail).unwrap();
        conflicting.page("duplicate", "Duplicate", List).unwrap();
        assert_eq!(
            installed.merge(conflicting).unwrap_err().code(),
            ErrorCode::Conflict
        );
        assert_eq!(installed.module("test").actions(), &["review.list"]);
        let mut added = ConsoleExtension::new();
        added.page("new", "New", Detail).unwrap();
        installed.merge(added).unwrap();
        let mut catalog = Catalog::new();
        catalog.register(installed.module("test")).unwrap();
        assert_eq!(
            installed
                .manifest(&catalog)
                .unwrap()
                .pages
                .iter()
                .map(|page| page.id.as_str())
                .collect::<Vec<_>>(),
            ["base", "new"]
        );
        let context = Context {
            call_id: "merge".into(),
            principal: Some(Principal {
                subject: "alice".into(),
                scopes: vec!["read".into()],
            }),
            resources: (),
            cancellation: CancellationToken::new(),
        };
        assert_eq!(
            installed
                .call(
                    &Executor::new(|_| {}),
                    &context,
                    "review.detail",
                    serde_json::json!(4)
                )
                .await
                .unwrap(),
            4
        );
    }
    #[tokio::test]
    async fn standalone_operations_execute_without_a_page_control() {
        let mut extension = ConsoleExtension::new();
        extension.register(List).unwrap();
        assert_eq!(
            extension.register(List).unwrap_err().code(),
            ErrorCode::Conflict
        );
        let mut catalog = Catalog::new();
        catalog.register(extension.module("session")).unwrap();
        assert!(extension.manifest(&catalog).unwrap().pages.is_empty());
        assert_eq!(
            extension.manifest(&Catalog::new()).unwrap_err().code(),
            ErrorCode::NotFound
        );
        let mut context = Context {
            call_id: "standalone".into(),
            principal: None,
            resources: (),
            cancellation: CancellationToken::new(),
        };
        let executor = Executor::new(|_| {});
        assert_eq!(
            extension
                .call(&executor, &context, "review.list", serde_json::Value::Null)
                .await
                .unwrap_err()
                .code(),
            ErrorCode::Unauthenticated
        );
        context.principal = Some(Principal {
            subject: "alice".into(),
            scopes: vec!["read".into()],
        });
        assert_eq!(
            extension
                .call(&executor, &context, "review.list", serde_json::Value::Null)
                .await
                .unwrap(),
            serde_json::json!([1])
        );
    }
}
