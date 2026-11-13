//! Shared native and external action conformance tests.

use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use lenso_app::{
    Action, ActionError, CancellationToken, Context, ErrorCode, Executor, Principal, require_owner,
    require_scope,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Input {
    owner: String,
    value: i64,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    input: serde_json::Value,
    subject: Option<String>,
    scopes: Vec<String>,
    mode: String,
    expected: String,
    runs: usize,
}

struct ReadFile<'a> {
    mode: &'a str,
    runs: AtomicUsize,
}

impl Action<()> for ReadFile<'_> {
    type Input = Input;
    type Output = i64;
    fn id(&self) -> &str {
        "files.read"
    }
    fn validate_input(&self, input: &Input) -> Result<(), ActionError> {
        if input.value < 0 {
            Err(ActionError::new(ErrorCode::InvalidInput, "Negative value"))
        } else {
            Ok(())
        }
    }
    async fn authorize(&self, context: &Context<()>, input: &Input) -> Result<(), ActionError> {
        require_scope(context.principal.as_ref(), "files:read")?;
        require_owner(context.principal.as_ref(), &input.owner)?;
        if self.mode == "cancel_authorize" {
            context.cancellation.cancel();
        }
        Ok(())
    }
    async fn run(&self, context: &Context<()>, input: Input) -> Result<i64, ActionError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        if self.mode == "fail" {
            return Err(ActionError::new(ErrorCode::Internal, "Action failed")
                .with_source(std::io::Error::other("private dependency details")));
        }
        if self.mode.starts_with("cancel_") {
            context.cancellation.cancel();
        }
        if self.mode == "cancel_handler" {
            context.check_cancelled()?;
        }
        Ok(if self.mode == "invalid_output" {
            -1
        } else {
            input.value
        })
    }
    fn validate_output(&self, output: &i64) -> Result<(), ActionError> {
        if self.mode == "invalid_output_code" {
            return Err(ActionError::new(
                ErrorCode::Forbidden,
                "Invalid output implementation",
            ));
        }
        if *output < 0 {
            Err(ActionError::new(ErrorCode::Internal, "Invalid output"))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn shared_action_semantics() {
    let fixtures: Vec<Fixture> =
        serde_json::from_str(include_str!("../../../../conformance/fixtures/action.json")).unwrap();
    for fixture in fixtures {
        for external in [false, true] {
            if external && !cfg!(feature = "json") {
                continue;
            }
            if !external && fixture.name == "malformed external input" {
                continue;
            }
            let records = Mutex::new(Vec::new());
            let executor = Executor::new(|record| records.lock().unwrap().push(record));
            let action = ReadFile {
                mode: &fixture.mode,
                runs: AtomicUsize::new(0),
            };
            let context = Context {
                call_id: "call-1".to_owned(),
                principal: fixture.subject.as_ref().map(|subject| Principal {
                    subject: subject.clone(),
                    scopes: fixture.scopes.clone(),
                }),
                resources: (),
                cancellation: CancellationToken::new(),
            };
            if fixture.mode == "cancel_before" {
                context.cancellation.cancel();
            }
            let result = if external {
                #[cfg(feature = "json")]
                {
                    executor
                        .call_json(&action, &context, fixture.input.clone())
                        .await
                        .map(|value| value.as_i64().unwrap())
                }
                #[cfg(not(feature = "json"))]
                {
                    unreachable!("JSON feature checked above");
                }
            } else {
                executor
                    .call(
                        &action,
                        &context,
                        serde_json::from_value(fixture.input.clone()).unwrap(),
                    )
                    .await
            };
            let outcome = result
                .as_ref()
                .map(|_| "ok")
                .unwrap_or_else(|error| error.code().as_str());
            assert_eq!(
                outcome, fixture.expected,
                "{} external={external}",
                fixture.name
            );
            if let Ok(output) = result {
                assert_eq!(output, fixture.input["value"].as_i64().unwrap());
            }
            assert_eq!(
                action.runs.load(Ordering::SeqCst),
                fixture.runs,
                "{}",
                fixture.name
            );
            let records = records.lock().unwrap();
            assert_eq!(records.len(), 1, "{}", fixture.name);
            assert_eq!(
                records[0]
                    .outcome
                    .map(|_| "ok")
                    .unwrap_or_else(|code| code.as_str()),
                fixture.expected
            );
            assert_eq!(records[0].subject, fixture.subject);
        }
    }
}

struct Native;
struct NativeValue {
    callback: fn() -> u128,
}
impl Action<()> for Native {
    type Input = NativeValue;
    type Output = NativeValue;
    fn id(&self) -> &str {
        "native.identity"
    }
    async fn authorize(&self, _: &Context<()>, _: &NativeValue) -> Result<(), ActionError> {
        Ok(())
    }
    async fn run(&self, _: &Context<()>, value: NativeValue) -> Result<NativeValue, ActionError> {
        Ok(value)
    }
}

#[tokio::test]
async fn native_types_and_send_future() {
    fn assert_send<T: Send>(_: &T) {}
    let context = Context {
        call_id: "native-1".into(),
        principal: None,
        resources: (),
        cancellation: CancellationToken::new(),
    };
    let executor = Executor::new(|_| {});
    let future = executor.call(
        &Native,
        &context,
        NativeValue {
            callback: || u128::MAX,
        },
    );
    assert_send(&future);
    let output = future.await.unwrap();
    assert_eq!((output.callback)(), u128::MAX);
}
