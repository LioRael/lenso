# Make App Composition the Capability binding authority

App Composition will resolve every declared Capability requirement before activation. Requirements declare `one`, `optional`, or `many` cardinality and bind deterministically to App-local Module keys; a Module receives only the Capability handles selected for its declared requirements and cannot query a global Registry for undeclared dependencies.

## Consequences

- The same Module package may appear several times under distinct App-local keys and configuration.
- `one` requirements fail composition when missing or ambiguous, `optional` requirements may be absent, and `many` requirements receive a deterministic provider order.
- The first release rejects cycles formed by required request or stream activation dependencies. Event subscriptions and optional or many observation edges are validated but do not impose activation order.
- The static graph is both the dependency model and the initial least-authority model. Trusted native code can technically escape process-level restrictions, but the supported Module Interface supplies no bypass.
