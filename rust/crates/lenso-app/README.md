# lenso-app

Native Rust application primitives. No CLI, Console process, JSON round trip or cross-language host is needed for native execution.

```rust
use lenso_app::{Action, ActionError, CancellationToken, Context, Executor};

struct Double;

impl Action<()> for Double {
    type Input = u64;
    type Output = u64;

    fn id(&self) -> &str { "numbers.double" }

    async fn authorize(&self, _: &Context<()>, _: &u64) -> Result<(), ActionError> {
        Ok(())
    }

    async fn run(&self, _: &Context<()>, value: u64) -> Result<u64, ActionError> {
        value.checked_mul(2).ok_or_else(|| {
            ActionError::new(lenso_app::ErrorCode::InvalidInput, "Number is too large")
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context {
        call_id: "example".into(), principal: None, resources: (),
        cancellation: CancellationToken::new(),
    };
    let executor = Executor::new(|_record| {});
    assert_eq!(executor.call(&Double, &context, 21).await?, 42);
    Ok(())
}
```

Enable `json` for `call_json`, durable `Task` definitions and serialized Console descriptions. Native actions have no serde bounds. Resources retain their original library interfaces and Rust ownership; modules describe explicit resource requirements. The observer must not panic. Cancellation is cooperative and does not imply transaction rollback.

`Resource` definitions validate native configuration before acquisition. `connect` returns the original connection or client. `with_resource` awaits explicit asynchronous cleanup after success or a returned error, preserving simultaneous use and close failures in `ResourceError::UseAndClose`. Ordinary constructors and RAII remain usable directly.

`all_of` and `any_of` compose synchronous native checks; `all_of_async` and `any_of_async` lazily compose asynchronous checks. Alternatives handle access denials only, preserving dependency failures. The `json` feature also enables typed `DurableEvent` subscriptions and `ConsoleExtension` bindings for list, detail and custom management actions. These bindings reuse the normal executor and derive their Console descriptions from the actual installed actions.

Independent modules can each return a `ConsoleExtension`. Derive their catalog module with `extension.module("name")`, attach non-Console exports/resource requirements, then combine extensions with `merge`. Merge preserves order and rejects duplicate pages or operations before changing the receiving extension. See the Team Desk example for composition without a second Action directory.

Use `extension.register(action)` for an authorized operation that is not offered as a page control, such as the current-session query. It is included in `extension.module(...)` and shares the ordinary Executor pipeline. Manifest construction checks these standalone operation references too.
