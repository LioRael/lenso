# Lenso

`lenso` is the public Rust facade for Lenso module-authoring and host boot
contracts.

Install it from crates.io:

```sh
cargo add lenso@0.3.18
```

The default facade exposes serializable module manifest declarations:

- module manifests and manifest lints;
- HTTP route metadata;
- runtime function declarations;
- event handler declarations;
- lifecycle declarations;
- Console surface declarations;
- story display metadata.

Enable the `host` feature for the narrow host boot facade:

```sh
cargo add lenso --features host
cargo add async-trait serde_json
cargo add tokio --features macros,rt-multi-thread
```

Application SQL, repositories, auth/session policy, CRUD shape, and Runtime
Console UI stay in the host application or module code.

Manifest-declared behavior for a host-owned linked Module is authored through
`lenso::host::runtime`. It exposes the linked loader, binding, function handler,
descriptor, retry, execution context, and standard error types needed to return
a behavior-bearing `HostLinkedModule`; external Modules do not import a
`lenso-platform-*` crate.

```rust
use async_trait::async_trait;
use lenso::host::runtime::{
    AppContext, AppResult, ExecutionContext, FunctionDefinition, FunctionHandler,
    LinkedBinding, Module, RetryPolicy, RuntimeDescriptor,
};
use lenso::host::{HostBuilder, HostLinkedModule, Migration};
use lenso::{
    ModuleManifest, RuntimeFunctionDeclaration, RuntimeRetryPolicyDeclaration, RuntimeSurface,
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

const FUNCTION_NAME: &str = "inventory.reconcile.v1";
const MIGRATIONS: &[Migration] = &[];

#[derive(Debug)]
struct Reconcile;

#[async_trait]
impl FunctionHandler for Reconcile {
    async fn call(&self, _context: ExecutionContext, input: Value) -> AppResult<Value> {
        Ok(input)
    }
}

fn manifest() -> ModuleManifest {
    ModuleManifest::builder("example/inventory")
        .runtime(RuntimeSurface {
            functions: vec![RuntimeFunctionDeclaration {
                name: FUNCTION_NAME.to_owned(),
                version: 1,
                queue: "inventory".to_owned(),
                input_schema: None,
                retry_policy: Some(RuntimeRetryPolicyDeclaration {
                    max_attempts: 3,
                    initial_delay_ms: 5_000,
                }),
                operation: None,
            }],
            schedules: Vec::new(),
            workflows: Vec::new(),
        })
        .build()
}

fn load(_context: &AppContext) -> Module {
    Module::linked(
        manifest(),
        LinkedBinding::builder()
            .runtime(RuntimeDescriptor {
                module: "inventory",
                functions: vec![FunctionDefinition {
                    name: FUNCTION_NAME.to_owned(),
                    version: 1,
                    queue: "inventory".to_owned(),
                    retry_policy: RetryPolicy::fixed(3, Duration::from_secs(5)),
                    handler: Arc::new(Reconcile),
                }],
                ..RuntimeDescriptor::default()
            })
            .build(),
    )
}

fn linked_module() -> HostLinkedModule {
    HostLinkedModule::linked("inventory", manifest, load, MIGRATIONS)
}

#[tokio::main]
async fn main() {
    HostBuilder::new()
        .linked_module(linked_module())
        .run_worker_from_env()
        .await
        .expect("run Lenso worker");
}
```

Host-owned linked modules can use `lenso::host::transaction` when one operation
must atomically claim an idempotency key, execute app-owned SQL, and publish an
Outbox event. The application still writes its business query with `sqlx`; it
does not import `lenso-platform-core` or address platform tables directly.

Consumers that only need this transaction boundary can avoid the complete Host
boot dependency graph:

```toml
lenso = { version = "0.3.19", features = ["host-transactions"] }
```

```rust,ignore
use lenso::host::transaction::{
    IdempotencyClaim, IdempotencyKey, LinkedTransaction, OutboxEvent,
};

let key = IdempotencyKey::parse("orders:create", request_key)?;
let mut transaction = LinkedTransaction::begin(&context.db).await?;
if transaction.claim_idempotency_key(&key).await? == IdempotencyClaim::Existing {
    transaction.rollback().await?;
    return Ok(());
}

sqlx::query("insert into orders (id) values ($1)")
    .bind(order_id)
    .execute(&mut **transaction.sql())
    .await?;
transaction.publish_outbox(&event).await?;
transaction.commit().await?;
```

The same feature exposes the host-owned relay through
`lenso::host::outbox`. A host implements `EventDispatcher` and passes it to
`OutboxRelay::relay_once`; it does not import a `lenso-platform-*` crate or
address Outbox tables directly. Delivery is at least once. When a dispatcher
returns a retryable `AppError`, the existing host retry and dead-letter policy
decides when to redeliver or exhaust the event. Consumers must therefore make
effects idempotent using the stable `ClaimedOutboxEvent::id`.

```rust,ignore
use lenso::host::outbox::{
    AppError, AppResult, ClaimedOutboxEvent, ErrorCode, EventDispatcher,
    OutboxRelay,
};

#[derive(Debug)]
struct Consumer;

#[async_trait::async_trait]
impl EventDispatcher for Consumer {
    async fn dispatch(&self, event: &ClaimedOutboxEvent) -> AppResult<()> {
        consume_idempotently(&event.id, &event.payload)
            .await
            .map_err(|error| {
                AppError::new(ErrorCode::ExternalDependency, "consumer unavailable")
                    .with_source(error)
                    .retryable()
            })
    }
}

let relay = OutboxRelay::new(context.db.clone(), "app-worker");
relay.relay_once(&Consumer, 25).await?;
```

## Example

```rust
use lenso::{ModuleManifest, lint_module_manifest};

let manifest = ModuleManifest::builder("example")
    .capabilities(vec!["example.records.read".to_owned()])
    .build();

let lints = lint_module_manifest(&manifest);
assert!(
    lints
        .iter()
        .all(|lint| !matches!(lint.severity, lenso::ModuleManifestLintSeverity::Error))
);
```
