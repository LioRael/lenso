# Team Desk authoring assessment — 2026-11-13

## What was built

The [application](../examples/team-desk/README.md) implements four independent business modules in each language: members, projects, work items and notifications. Modules supply native Actions and Console contributions. The host supplies identity, native persistence, records, HTTP and composition. Item changes and notification jobs commit in the same SQLite transaction. The producer owns the event contract; notification wiring lives in the composition root.

The Console-client workflow creates projects and items, assigns work, advances status, changes roles and archives projects. It uses the actual SDK dispatcher and network client; there is no separate management implementation. An independent [browser host](../typescript/examples/team-desk-console/README.md) now completes the read/write workflow against both backends with the same compiled page factories.

## Friction discovered and removed

| Observed authoring problem | Change | Evidence |
| --- | --- | --- |
| TS `Catalog.register<T>` inferred one concrete module from a heterogeneous union. An ordinary module-registration loop failed despite every element being a valid module. | Register accepts `Module<ModuleExports>`; each module still exposes its original concrete native types. | The actual application loop compiles; packed-consumer verification registers differently typed modules in one loop and still rejects fake Action descriptors. |
| TS required `consoleOperation(action)` for each list, detail and write binding even with no custom codec. | Page definitions accept Actions directly. Binding creation and repeated-Action reuse are owned by `ConsoleExtension`; the explicit wrapper remains for codecs. | Four application modules use direct bindings. Regression coverage checks shared bindings, validation, records and conflicting handlers. The original private-files example also uses the simpler path. |
| A custom Console codec could contribute unwanted inference to an Action's input/output types. | Codec arguments use `NoInfer`, taking their contracts from the Action. | A packed-consumer negative check rejects a decoder returning the wrong native input. Another checks the direct binding's resource contract. |
| Independent Rust modules had to register against one mutable global Console and repeat their Action lists in Module descriptions. | `ConsoleExtension::merge` installs independent extensions atomically. Each module derives its description from its own actual bindings and adds only non-Console exports and requirements. | The application assembles a homogeneous array of module/extension pairs. A regression checks that a conflict leaves the receiver unchanged, and that successful merges retain declaration order and executable bindings. |
| Application permission calls with multiple boolean arguments obscured intent. | The TS host uses named permission modes; Rust uses a local enum. | Business methods read `access(context, "admin")` or `Permission::Write`. This stays application-owned because these roles are not universal SDK policy. |

The TS page-array composition retains one `flatMap<ConsolePageDefinition<Store>>` type argument for heterogeneous contributions. It is an ordinary collection annotation, not a reason to add an Application builder or erase the native operation types.

## What did not become an abstraction

- Native SQL and transactions remain visible. TS uses one synchronous connection and explicit scopes; Rust uses borrowed transactions within the queue's connection closure. Both publication paths join the domain transaction.
- Roles, tenancy, valid assignments, version checks and state transitions remain business rules in the application. They are not new SDK flags.
- Rust Action implementations retain associated input/output types and ordinary structs. A macro solely to hide trait syntax was not introduced.
- Record sinks own and log their persistence failures as required by the existing infallible observer contract. No transactional-audit guarantee was added.
- HTTP authentication/body handling, schema initialization and worker hosting remain explicit application responsibilities. Existing SDK mechanisms did not justify a universal transport or dependency container.

## Verified experience

The full integration suite has 19 passing cases, including twelve Team Desk cases. Actual process tests cover successful Console writes, denied writes, immediate stored-role changes, cross-team isolation, invalid assignments/statuses, archive behavior, stale versions, restart and pending-event interchange in both directions. A trigger aborts job insertion and proves the domain write is rolled back. A TS server and a Rust server also run concurrently on the same database, competing on one version and consuming the shared notification queue.

Native TS calls preserve concrete module exports. The Rust application test constructs native inputs directly and receives typed domain results without serialization. Both test independently installing only the projects Console module. SDK-level tests and packed-consumer checks cover the public-interface changes. Exact tool/environment results are in [verification](verification.md).

## Revised judgment

The [review based on the browser consumer](browser-consumer-sdk-review-2026-11-13.md) found protocol and ordering defects beyond the previously passing success flows. Their correction improves the common path, but also demonstrates why the maturity assessment must remain conservative.

| Dimension | Current assessment | Evidence and remaining cost |
| --- | --- | --- |
| Elegant | Coherent native core and a cleaner consumer interface | The executor remains the execution owner. Parsed client results now have one response/error owner; Console references use shared validation rather than a second handler directory. Rust ownership and asynchronous cleanup remain explicit rather than hidden behind macros. |
| Practical | Ready to start real applications in the demonstrated scope | The same browser pages work against TS/Rust, including denied writes, revocation, stale versions, malformed results and cancellation. Production identity, deployment, retention and long-running worker operations remain unverified application work. |
| Simple | Common paths are concise; distributed effects still require understanding | The host no longer needs a result-parsing query wrapper or separate session parsing. Raw and parsed client calls serve known versus unknown result contracts. Input validation stays application-owned; transaction, retry and idempotency semantics are explicit. No mandatory global Application/View/session abstraction was added. |
| Flexible | Actual replacement seams, with bounded evidence | Native resources, transports and structural parsers are replaceable; raw unknown calls remain available. Browser views are not tied to a framework. The session-order fix stays local. Another provider, a production auth integration and broader runtime support still need their own implementation evidence. |
| Powerful | Useful correctness mechanisms, not platform completeness | Native transactions, typed submission, transactional fanout, fenced leases, history and policies already work. The client now distinguishes server, input, protocol, result and cancellation phases. None of these implies rollback after a bad response, forced termination, exactly-once effects, distributed replay or production throughput. |

The remaining cost is mostly explicit native ownership, application policy and operational semantics. Adding many declarations or a universal framework would not remove those responsibilities. Conversely, the previous browser result-parsing wrapper did conceal a real ownership problem, so its semantics now belong to the SDK's optional parsed call rather than being reimplemented by every page host.

Overall: this is a useful preview SDK with evidence for elegant and simple common paths, not a mature general backend platform. Keep adoption focused on real consumers and failure contracts rather than equating feature counts or passing tests with universal readiness.

## Browser consumer follow-up

Connecting a real browser exposed a default-transport receiver bug that Node execution did not reveal. ConsoleClient now calls both default and injected fetch transports with the global receiver. A failed-first regression and actual browser flows cover this. The other SDK addition is a small explicit standalone-operation interface, used for current-session context and the notification worker. Both languages preserve the same Executor and catalog-validation rules.

Application schemas were extracted into a browser-safe contract entrypoint so forms do not duplicate server validation or import native SQLite code. View lifetime, submission state and conflict recovery remain host-owned, using native elements and AbortControllers rather than a new SDK state-management framework. Records and pages have separate request scopes; an old committed write cannot repaint a new editor. Cancelling its request is not represented as rolling back its effects.

The assessment improves most in **practicality** and **flexibility**: actual browser consumers now work with either backend. **Elegance** and **simplicity** benefit from shared contracts and explicit operation bindings, while the SDK still avoids dictating a frontend framework. **Power** remains grounded in the exercised authorization, version and persistence mechanisms; it does not imply production hosting, full Official Console completeness or universal browser/platform compatibility. See the verification record for current engine and test evidence.

The latest browser-consumer review further corrects malformed-response classification, URL/query routing, cancellation checks, manifest invariants and concurrent session ordering. The latter is host state, not SDK identity policy. Its successful browser regressions and the updated judgment above supersede earlier present-tense maturity claims.
