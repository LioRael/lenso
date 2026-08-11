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

```toml
lenso = { version = "0.3.18", features = ["host"] }
```

Application SQL, repositories, auth/session policy, CRUD shape, and Runtime
Console UI stay in the host application or module code.

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
