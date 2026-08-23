# Web and UI Module recipe

Use this branch when a Module owns HTTP-facing product behavior or contributes
target-owned App UI. Separate protocol behavior from host mechanics and keep
all Capability access explicit in App Composition.

## Backend HTTP behavior

A business Module can provide `lenso.http.endpoint@1`; the selected Web Ingress
Module consumes `many` endpoint providers and owns listener, request limits,
transport parsing, cancellation, and protocol response mapping. The business
endpoint owns authentication orchestration, request decoding, Capability
calls, final authorization, and intentional HTTP responses.

With the current `LioRael/lenso-web` SDK, declare routes and dispatch from one
source:

```rust,ignore
#[derive(Clone, Debug)]
struct OrdersHttp;

impl OrdersHttp {
    async fn create(
        &self,
        context: InvocationContext,
        request: HandleRequest,
    ) -> Result<HandleResponse, EndpointHandleInvocationError> {
        // Validate credential evidence, call the Orders Capability, and map
        // its typed outcome to an intentional HTTP response.
    }
}

http_endpoint! {
    impl OrdersHttp {
        "orders.create" => ("POST", "/orders") => create,
    }
}
```

Wrap the generated `EndpointEndpoint` in the Module's native factory, declare
the endpoint Capability on that Instance, declare a `many` requirement on the
Web Ingress Instance, and bind each endpoint provider explicitly. Route
collisions must fail before readiness.

Read `LioRael/lenso-web` `README.md`,
`crates/lenso-capability-http-endpoint/src/authoring.rs`, and
`crates/lenso-web-ingress/tests/http_ingress.rs` for the selected version's
complete factory and Composition example.

## Target-owned App UI

A target-owned Web UI is an ordinary composition of:

- a Web Shell Module requiring `many lenso.ui.contribution@1`;
- a Browser Adapter Module requiring exactly one `lenso.web.shell@1` and the
  portable business Capabilities projected to browser clients;
- one or more UI Contribution Modules providing route/navigation/asset
  metadata plus their declared portable business requirements; and
- the business Modules that provide those requirements.

One package may publish separate `backend` and `ui` entrypoints. Select them as
separate keyed Module Instances so either can be removed independently. Mark
their authoring roles as `web_shell`, `browser_adapter`, or `ui_contribution`
only when the installed authoring schema supports those roles.

The Browser Adapter exposes generated clients only for requirements declared
by the selected contribution and bound before boot. Same-realm UI code is
trusted application code, not a security sandbox. An independent cross-App
operator product is a separate App only when it has its own trust domain,
targets, durable state, or release lifecycle.

## Proof

For backend HTTP work, cross the real listener and endpoint Capability path.
For UI work, render the contributed route and invoke the generated browser
client through the Browser Adapter. Then remove the HTTP endpoint or UI
entrypoint and confirm the remaining App still resolves and its non-Web
Capabilities continue to work.
