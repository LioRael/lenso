# Provider Service Fixture

Small out-of-process Service fixture for the Lenso Provider Protocol.

It exposes a read-only CRM-style Contacts module:

- `GET /lenso/provider/v1/manifest`
- `GET /lenso/provider/v1/admin/contacts?limit=50&cursor=...`
- `GET /lenso/provider/v1/admin/contacts/{id}`

The default manifest also declares a host-rendered Console surface:

- package: `@lenso/provider-crm-console`
- export: `providerCrmConsoleModule`
- route: `/data/provider-crm`

The frontend package itself is owned by the Console repository. This
backend fixture only declares the surface metadata that the host API exposes.

It also exposes a second embedded-admin module base for testing
`AdminSurface::EmbeddedCustom`:

- `GET /lenso/provider/v1/embedded/manifest`
- `GET /lenso/provider/v1/embedded/admin`

And a declarative custom admin module base for testing
`AdminSurface::DeclarativeCustom`:

- `GET /lenso/provider/v1/declarative/manifest`
- `GET /lenso/provider/v1/declarative/admin/contacts?limit=50&cursor=...`
- `GET /lenso/provider/v1/declarative/admin/contacts/{id}`

Run it locally:

```sh
cargo run --locked -p provider-fixture
```

The server listens on `127.0.0.1:4100` by default. Override it with:

```sh
LENSO_SERVICE_ADDR=127.0.0.1:4101 cargo run --locked -p provider-fixture
```

Run the same fixture as a native gRPC Provider Service:

```sh
cargo run --locked -p provider-fixture -- --grpc
```

Connect it through Console and the target-owned System Plane management
interface: preview and approve an exact Module Change Plan, apply its Service
Installation Plan with either the HTTP or gRPC endpoint binding, then restart
API and worker. Provider endpoints are never discovered from environment
variables.

The API loads the module manifest at startup. The HTTP transport also serves
schema-admin data through the normal `/admin/data/*` backend; the gRPC transport
currently covers manifest, runtime function, and event-handler calls.
The manifest also declares module-local HTTP route metadata for `/contacts`,
`/contacts/{id}`, and proxy fixture routes. The host preserves that metadata
under `/admin/data/modules` and exposes matched routes through:

```text
/modules/provider-crm/http/{*path}
```

Proxy calls are persisted in `platform.provider_http_proxy_calls` with
request/correlation/trace/span context. Console shows them in the
horizontal Provider Calls page, as `provider_proxy_call` nodes in Runtime Story
graph/timeline views, and as `source = "provider_proxy"` rows in Technical
Operations.

## Runtime Story Verification

Use this manual cross-service flow when checking that the provider HTTP proxy is
visible from the Runtime Story perspective.

From the repo root, start Postgres and migrations, then run the Provider Service
fixture and API in separate shells:

```sh
docker compose -f ../../infrastructure/local/docker-compose.yml up -d postgres
cargo run --locked -p lenso-migrate
cargo run --locked -p provider-fixture
# Apply the reviewed Module and Service Installation plans in Console.
cargo run --locked -p lenso-api
```

Seed and verify the provider story path with the direct HTTP request below. The
Runtime Console in the sibling repository can then inspect the resulting
Provider Call, Runtime Story, and Technical Operations evidence.

The host path after `/modules/provider-crm/http` is matched against the module
manifest route `/contacts/{id}`. A path such as `/contact_1` or a token missing
`provider_crm.contacts.read` will not hit this declared route.

In Console, verify:

- Provider Calls contains `corr_console_api_fixture`.
- Stories contains `corr_console_api_fixture` with a `Provider Call` timeline row.
- The row summary shows `ok / provider-crm / GET /contacts/{id} / status 200`.
- Selecting the provider call node shows request, trace, span, path params, and
  route details in the Inspector.
- Technical Operations includes a row with `source = provider_proxy`.

Then trigger a successful proxied contact fetch:

```sh
curl \
  -H "Authorization: Bearer dev-service:admin:provider_crm.contacts.read" \
  -H "x-request-id: req_demo_provider_story_1" \
  -H "x-correlation-id: corr_demo_provider_story_1" \
  http://localhost:3000/modules/provider-crm/http/contacts/contact_1
```

Trigger a failed provider response that is still recorded as a provider proxy call:

```sh
curl \
  -H "Authorization: Bearer dev-service:admin:provider_crm.contacts.read" \
  -H "x-request-id: req_demo_provider_story_2" \
  -H "x-correlation-id: corr_demo_provider_story_2" \
  http://localhost:3000/modules/provider-crm/http/proxy-fixtures/text
```

The failure request creates a failed `provider_proxy_call` node and keeps its
provider error details in Inspector and Technical Operations.

To load the schema-admin, embedded iframe, and declarative exports, select all
three exact Module Releases in the reviewed Module Change Plan. The Provider
descriptor may verify those selections but cannot discover or enable sibling
exports.

The embedded manifest points at the example's `/embedded/admin` page with an
origin allowlist for the current request host, so the Console can render
it in a sandboxed iframe without a host bridge.
The declarative manifest uses host-rendered `metric_strip`, `entity_table`, and
`entity_detail` sections backed by the same Contacts fallback schema. The table
and detail sections are read-only and use the declarative admin data endpoints
above; the fallback schema is not advertised as a generic schema-admin module.

Use the direct HTTP requests above for the Provider Calls and Runtime Story
verification flow after the backend services are running. The resulting
evidence is inspected in the sibling Runtime Console; it is not a fixture-local
test.
