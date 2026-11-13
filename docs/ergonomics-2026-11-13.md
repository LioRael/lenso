# SDK ergonomics assessment — 2026-11-13

This is a source and authoring-path assessment after the design corrections in the [review](review-2026-11-13.md). It is not a user study or production-readiness certification. Executed behavior checks are recorded separately in [verification](verification.md).

Follow-up: the proposed multi-module authoring exercise is now implemented as [Team Desk](../examples/team-desk/README.md). Its [assessment](team-desk-assessment-2026-11-13.md) records the composition and Console friction found and the resulting SDK changes. The original assessment below remains the baseline for that exercise. The later [SDK review](post-team-sdk-review-2026-11-13.md) and [revised assessment](team-desk-assessment-2026-11-13.md#revised-judgment) supersede its present-tense verdict.

## Judgment

The SDKs are suitable for an initial real application. They provide useful execution and persistence mechanisms without requiring an application host, generated clients or an erased dependency container. They are not yet proven as mature, broadly convenient application SDKs. More abstraction would currently increase the interface to learn faster than it would remove application work.

| Quality | Assessment | Concrete basis and remaining cost |
| --- | --- | --- |
| Elegant | Mostly achieved in the core | One executor owns Action validation, access, cancellation and records. Native calls retain native values. Console and durable subscriptions reuse the existing operations. Dynamic Console bindings and Rust async cleanup are less compact than ordinary native calls. |
| Practical | Demonstrated for the reference workflows | Private files, protected management reads, invitation tasks and registration notifications run against real persistence. Production identity, deployment operations and non-SQLite storage remain unproven. |
| Simple | Core is small to adopt; the whole surface is not trivial | A native Action needs a definition, trusted context and executor. Catalog, configured resources, durable queues and Console are optional. Understanding leases, retries and transaction ownership remains necessary when adopting durable execution. |
| Flexible | Good within explicit native contracts | Resources can be ordinary clients, schemas are structural and codecs can change wire formats. Rust native Actions need no serde. A custom queue provider would currently require its own native integration; there is no demonstrated portable provider interface. |
| Powerful | Substantial mechanism coverage, limited provider breadth | Transactional fanout, idempotent submission, fenced leases, retries, history and policy composition handle useful failure modes. This does not establish distributed event replay, production operations, throughput or exactly-once effects. |

## Adoption paths

1. **Native operation:** define the input/output and access check, provide native resources in a trusted context, call the executor. No Module, Catalog, Console or queue is required.
2. **External operation:** reuse the Action's TS parser or Rust serde conversion. Add a custom TS codec only for a different external representation. The HTTP adapter still owns authentication, body handling and response mapping.
3. **Durable work:** define a serialized Task, enqueue using the native driver, run a worker and make effects idempotent. A durable event adds subscriber mapping without making the producer name tasks. Use the same native transaction for business writes and publication.
4. **Management:** bind existing Actions to Console pages. A compiled frontend view validates the dynamic result before rendering. The manifest cannot supply executable frontend code.

The private-files TS example exercises inferred schema-based definitions. The workflow example intentionally also shows typed native Action objects without schemas. Those are two supported entrypoints, not a requirement to repeat DTO declarations after defining a schema.

## Costs to retain explicitly

- **Authorization is explicit.** Public Actions still declare an authorization method; adding a permissive default would hide a security decision. Ownership must be checked against trusted stored data. The app README example was corrected to avoid trusting an input owner.
- **Parsing has a contract.** A TS parser used by both entrypoints must accept the native type as well as its external input. Type-changing transport conversion belongs in a codec. Declaring both a parser and an invariant hook is optional, not the standard recipe.
- **Records are observability hooks.** TS awaits its sink; Rust uses a synchronous sink whose latency is application-owned. Both require the application to contain sink failures. Neither is a transactional audit log. A durable audit mechanism needs its own transaction design.
- **Async cleanup has a lifetime cost.** Rust `with_resource` borrows the resource through a boxed future so cleanup can retain ownership. Ordinary native constructors and RAII remain the simpler choice when explicit asynchronous close is unnecessary. Neither scope API promises async cleanup after process termination; Rust future cancellation also does not await it.
- **Queue operation requires agreement.** Workers for a task kind must agree on concurrency. Retention, worker hosting and deployment configuration are application work. Native SQLite connection access in Rust holds a lock and must not reenter the queue. These constraints are part of usability, even when correctness tests pass.
- **Dynamic Console results stay unknown.** Page extensions must validate their result. A generic type assertion would shorten code without adding safety. The cost should be reassessed when real built-in views and a second application exist.

## Restraint and next evidence

Keep the native APIs and optional composition. Do not add a global Application builder, automatic dependency injection, an Action/Task supertype, cross-language code generation or a universal storage-provider interface solely to make the feature list look uniform. Local and durable events deserve distinct types because their failure and delivery contracts differ.

Before freezing the public interface, implement a second application with several independently authored modules and at least one management write flow. Observe repeated caller work there: resource startup, policy reuse, request adaptation, result validation and worker setup. Extract helpers when that repeated work has the same semantics. Provider portability needs a second concrete provider; maturity needs deployment and operational evidence. More declarations or more unit tests alone do not establish either.
