# Payments

`@lenso/payments` persists provider payment/refund requests and verified results. It does not price products, grant memberships, maintain a wallet or implement an accounting ledger.

## Entries and dependencies

The ordinary async core (`createPayments`, contracts and safe errors) has no external runtime dependencies. Install optional peers only for the selected entries:

| Entry                            | Purpose                                                       | Optional peers               |
| -------------------------------- | ------------------------------------------------------------- | ---------------------------- |
| root                             | Services, durable store/provider contracts                    | None                         |
| `/stripe`                        | Official Stripe SDK PaymentIntent/refund adapter              | `stripe@23.0.0`              |
| `/plugin`                        | Exact installed store/provider/authorization dependencies     | `@lenso/core`                |
| `/auth`                          | Auth provenance, audience and resource policy enforcement     | `@lenso/auth`                |
| `/config`                        | Lenso Config contract for leases, scan delay and refund count | `@lenso/core`, `zod`         |
| `/tasks`                         | Existing Tasks queue, retries and scheduled continuation      | `@lenso/tasks`, `zod`        |
| `/scheduler`                     | Explicit recurring recovery plan using the existing Scheduler | Scheduler, Auth, Tasks types |
| `/fetch`                         | Bounded raw-body webhook ingress                              | None                         |
| `/manage`                        | Explicit read-only status companion                           | Core, Engine, Manage, `zod`  |
| `/drizzle/pg`, `/drizzle/sqlite` | PostgreSQL / Bun SQLite aggregate CAS                         | `drizzle-orm`                |
| `/drizzle/d1`                    | Borrowed raw D1 binding, primary reads                        | Drizzle, Workers types       |

The package and pinned optional peers are recorded in the single workspace lockfile. Future dependency updates belong to the integration owner; do not create a package-local lockfile.

## Minimal integration

Use the application's existing Drizzle database, Auth access and Stripe client. Apply `migrations/postgres.sql` or `migrations/sqlite.sql` explicitly through the application's migration workflow, never during service setup.

```ts
import { createPayments } from "@lenso/payments";
import { createStripeProvider } from "@lenso/payments/stripe";
import { postgresPaymentsStore } from "@lenso/payments/drizzle/pg";
import { paymentsAuthorization } from "@lenso/payments/auth";

// db, stripeClient, paymentsAccess, paymentPolicies and secrets are existing,
// trusted application dependencies. No credentials are accepted in business JSON.
const runtime = createPayments({
  store: postgresPaymentsStore(db),
  provider: createStripeProvider({
    client: stripeClient,
    accountId: expectedAccountId,
    live: false,
    webhookSecret: secrets.stripeEndpointSecret,
    // Product limits in Stripe API units, not global Stripe eligibility guarantees.
    currencies: { usd: { min: 50, max: 99_999_999 }, jpy: { min: 1, max: 99_999_999 } },
  }),
  authorize: paymentsAuthorization(paymentsAccess, paymentPolicies),
});
```

`paymentsAccess` is the **same Auth instance and audience** that authenticated the entry. Its membership reader loads trusted tenant permissions for the supplied `PaymentResource`. Policies are required for `create`, `read`, `client-secret`, `refund` and `results`. Creation must verify the business order/quote and amount; refund policy must require the application's refund permission, not just tenant membership. A copied actor, JSON identity, another Auth instance/audience or revoked credential cannot pass Auth enforcement.

```ts
const quote = await orders.paymentQuote(orderId, actor); // trusted server-side pricing
const payment = await runtime.payments.create(
  {
    tenantId: quote.tenantId,
    orderId: quote.orderId,
    key: quote.paymentRequestKey,
    amount: quote.amount,
    currency: quote.currency,
  },
  actor,
);
await queue.enqueue(reconcileTask, { limit: 50 });
// Only the authorized payer entry may return this capability to Stripe.js.
const clientSecret = await runtime.payments.clientSecret({ paymentId: payment.paymentId }, actor);
```

Creation is **unconfirmed**, with automatic capture and automatic payment methods. The application uses Stripe.js to confirm this same PaymentIntent and supply its trusted return URL. Creating an intent is not payment success. There is no parallel Checkout flow, server-side card handling, manual capture, Connect/organization routing, subscription, payout or transfer implementation.

Amounts are positive safe integers in Stripe's API units. No floating-point major-unit conversion is performed: USD `1000` and JPY `10` mean different unit scales. ISK/UGX require multiples of 100; HUF/TWD charge amounts do not inherit payout restrictions. Configure a lowercase currency allowlist and product bounds. Refunds require explicit positive amounts in the original currency and do not inherit the minimum charge amount.

For Lenso, `createPaymentsPlugin({ id, store, provider, authorization, config? })` takes exact installed plugin objects. Store/authorization companion plugins should require the application's exact DB/Auth instances and call `context.get` on those references. Resources belong to those owners; Payments does not close borrowed clients. Configuration may use `{ contract: paymentsConfig, sources: [...] }` with existing Lenso value/env/file sources. Provider secrets remain in the existing secret/config owner, with sensitive fields explicitly marked; the Payments operational contract contains no secrets.

## Durable receive, Tasks and scheduling

Mount `createPaymentsWebhookHandler` only at the configured Stripe endpoint. It retains raw bytes, bounds the body, verifies the official SDK signature with a nonzero time tolerance (default 300 seconds), checks account/mode/object/amount/currency, and commits a minimal inbox receipt before 2xx. It does not impersonate a browser actor. Database/provider failures return 503; bad signatures or foreign/mismatched objects return 400. Unrelated, correctly signed events are ignored. Subscribe this endpoint to PaymentIntent and `refund.created`, `refund.updated`, `refund.failed` snapshot events.

```ts
import { createPaymentsReconciliationTask } from "@lenso/payments/tasks";
import { createPaymentsWebhookHandler } from "@lenso/payments/fetch";

const reconcileTask = createPaymentsReconciliationTask({
  name: "payments.reconcile",
  runtime: () => runtime.reconciliation,
  queue: () => queue,
  continuation: false, // Scheduler below owns the recurring recovery cadence.
});
// Register this exact task in the existing queue before starting its worker.
const webhookHandler = createPaymentsWebhookHandler({
  webhook: runtime.webhook,
  wake: async () => {
    await queue.enqueue(reconcileTask, { limit: 50 });
  },
});
```

The trusted queue getter is resolved when the handler runs, after application assembly. Queue payloads contain only the batch limit, never actors, secrets or authorization grants. The handler's existing self-continuation remains enabled by default: it persists the next run through `TaskQueue.enqueue({runAt})`, with stable per-job/due-time keys and Tasks retries on enqueue failure. When using the recurring Scheduler plan, set `continuation: false` as above so cron does not keep adding independent polling chains; deferred work and remaining batches wait for the next scheduled sweep or webhook wake. Task failures still use the same retry policy. Tasks supplies its existing Log/OTel instrumentation; Payments never logs raw provider errors, signatures, client secrets or payment details.

**A recurring recovery plan is required**, to cover a crash between reservation/receipt and queue enqueue, or exhausted queue retries. Use the existing [`@lenso/scheduler`](../scheduler/README.md), with the same durable queue and exact reconciliation Task registered in both Scheduler and Tasks:

```ts
import { createPaymentsRecoverySchedule } from "@lenso/payments/scheduler";

// Explicit provisioning from a trusted maintenance entry, not application startup.
// scheduler is the existing service (or app.get(exactSchedulerPlugin)).
const recovery = await createPaymentsRecoverySchedule(
  {
    scheduler,
    task: reconcileTask,
    rule: { kind: "cron", expression: "*/5 * * * *", timezone: "UTC" },
    limit: 50,
  },
  maintenanceActor,
);
// Persist recovery.id in application-owned provisioning records. Reuse that ID
// with Scheduler's authorized get/update/pause/cancel, never recreate at every boot.
```

The adapter delegates cron validation, durable occurrences, queue identity binding, dispatch authorization and deduplication to Scheduler. It fixes misfire handling to `coalesce` (one catch-up sweep), defaults `limit` to 50 and `graceMs` to 5000, and never starts a timer or worker. Creation has Scheduler's ordinary create semantics, not an idempotent "ensure": each explicit call creates a new plan. An unknown creation response requires operator lookup through Scheduler before provisioning another.

`maintenanceActor` must come from the configured Auth instance/audience, not JSON. The Scheduler owner's `authorize` must use `Access.enforce` and check maintenance authority for this provider account/mode and task. Its `authorizeExecution` must recheck the persisted subject's current permission at every dispatch. This Task is a trusted account-wide recovery capability, not an ordinary tenant user's query/refund grant; its payload remains only `{limit}`. Do not expose plan creation or `tick` through Payments' read-only Manage surface.

Apply Scheduler and Tasks migrations through their existing explicit workflows. Use matching PostgreSQL stores/queue, or matching D1 stores/queue. The host still must drive Scheduler: on Bun, explicitly own `startSchedulerDriver` from `@lenso/scheduler/driver`, immediately register `driver.stop()` cleanup, and observe `driver.done`; on Workers, await finite `scheduler.tick()` and `queue.runBatch()` in the platform handler, with no perpetual loop. Webhook wake remains the fast path; the cron plan is the durable safety net. Choose cron frequency and batch limit for the expected recovery backlog.

This adapter consumes the Scheduler/Tasks public exports merged in `origin/main` at `7c35055`. An older checkout must use/build those merged dependencies before checking the adapter; it must not recreate their interfaces locally. No shared package implementation is modified here.

Webhook snapshots are not applied as state or ordered by timestamps. Their verified object IDs are retained so even an expired unknown operation can be recovered by provider GET. Worker queries the current provider state; terminal states cannot regress. Result IDs are persisted in the same aggregate CAS as state changes, so different events for one success do not produce another business result.

## Idempotency and database boundaries

- Payment keys are scoped by account, test/live mode and tenant. A business order has one payment record in that scope. Same key with a different order/amount/currency, or another key for that same order, rejects with `conflict`.
- Refund keys are scoped to the payment. Same key/different amount rejects. Unknown/pending/successful refunds reserve the cap in a single aggregate CAS, preventing concurrent local over-refunds. Verified failed/canceled refunds release their amount, but retain their key/result. A small documented set of definite Stripe refund rejections also becomes `refund.failed`; arbitrary 400s, timeouts, balance errors and 500s remain unknown.
- After an unknown response, public repeats return persisted state, not another POST. Workers query by known ID or fully paginated metadata match; only within the conservative 23-hour window may an unresolved request replay the same parameters/key. After that window, it remains unknown for verified webhook/GET or operator investigation. A scan truncation or multiple matches is not absence. Do not clear keys or issue a new payment just because a request timed out.
- PostgreSQL uses unique constraints and atomic `UPDATE ... WHERE revision ... RETURNING`. State, refund reservations and results share one bounded JSON aggregate. A surrounding DB transaction is possible with the supplied Drizzle transaction handle; no lock spans provider I/O.
- D1 uses single-statement conditional writes, not interactive transactions. Pass the **raw binding**, `d1PaymentsStore(env.DB)`, never a long-lived `withSession("first-primary")`: only its first query is forced to primary. According to the current D1 routing contract, non-Session queries use primary. Local D1 testing does not verify hosted replica routing.
- Leases reduce overlapping work; token checks fence stale **DB** observations. Processing renews an unexpired owned lease at each observation/refund step, and the adapter calls the DB write guard again after its preflight. Choose `leaseMs` above the longest individual provider query/recovery scan (not just one HTTP request); hosts need synchronized clocks, bounded SDK requests and worker drain. Leases cannot fence an arbitrarily suspended request at Stripe after its idempotency retention expires. No strict distributed consistency, cross-provider/DB atomic transaction or exactly-once claim is made.

Keep provider metadata immutable and use a dedicated SDK client for this account/sandbox. Requests disable SDK retries and use a 10-second per-request timeout (`requestTimeoutMs`, at most 60 seconds). The runtime verifies `/v1/account` and object mode. Stripe v23's `getApiField` guard rejects inherited account/context routing; that SDK configuration accessor is version-pinned and not a promise of compatibility with future SDK majors. Different sandboxes require their own credentials and endpoint secret even though all have `livemode: false`.

## Business results and Manage

Consume `payments.results({paymentId}, actor)` with the `results` policy. Each result contains only correlation, amount/currency, kind and stable `resultId`. In the application's own transaction, insert `resultId` into a unique consumed-results table and apply that business effect together. For external effects, pass that same stable key to an idempotent destination or maintain a durable application outbox. Reading a result is not acknowledgement or proof of delivery; Payments does not mutate entitlements.

`createPaymentsManage({id, payments: exactPaymentsPlugin})` is explicit opt-in and declares **only `status`**. Install its companion plugin and select its operations separately in CLI/MCP/HTTP allowlists. Supply `{actor}` from a trusted Auth entry binding; the shared `read` policy still enforces the loaded payment's tenant. No refund/write method, raw receipt, client secret or provider credential is exposed. No management ingress is enabled by default.

## Verification and remaining integration work

Focused tests use actual Bun SQLite (including reopen and two handles), local workerd D1 via Miniflare, and the official Stripe SDK with local HTTP/WebCrypto fixtures. The Scheduler test runs the merged D1 ScheduleStore and durable Tasks queue together with Payments D1 storage; only its external payment provider is a fixture. It covers a persisted unknown payment with no remaining wake/continuation, reassembled Scheduler recovery, unique results, forged Actor denial and revoked dispatch permission. These checks do not prove real Stripe or hosted D1 operation. No account, credential, real charge or refund is created.

PostgreSQL persistence and real Stripe account/API eligibility have **not** been exercised. Hosted Workers execution and D1 replicas are also unverified. Local Scheduler/Tasks recovery checks are separate from remote platform validation. Before deployment, the application owner must validate these in an authorized disposable environment, apply migrations, configure the endpoint/API version and recovery plan/host driver, and review its quote/refund/result-consumption policies. This task does not authorize deployment.

Official references used: [webhooks](https://docs.stripe.com/webhooks), [signature verification](https://docs.stripe.com/webhooks/signature), [PaymentIntent create](https://docs.stripe.com/api/payment_intents/create), [refund create](https://docs.stripe.com/api/refunds/create), [idempotency](https://docs.stripe.com/api/idempotent_requests), [error codes](https://docs.stripe.com/error-codes), [currencies](https://docs.stripe.com/currencies), [D1 read replication](https://developers.cloudflare.com/d1/best-practices/read-replication/). SDK `23.0.0` targets API `2026-09-30.endive`; align the configured snapshot endpoint version.
