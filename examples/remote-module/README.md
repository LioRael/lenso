# Remote Module Example

Small out-of-process module fixture for the Lenso Remote module protocol.

It exposes a read-only CRM-style Contacts module:

- `GET /lenso/module/v1/manifest`
- `GET /lenso/module/v1/admin/contacts?limit=50&cursor=...`
- `GET /lenso/module/v1/admin/contacts/{id}`

The default manifest also declares a host-rendered Runtime Console surface:

- package: `@lenso/remote-crm-console`
- export: `remoteCrmConsoleModule`
- route: `/data/remote-crm`

The frontend package itself is owned by the Runtime Console repository. This
backend fixture only declares the surface metadata that the host API exposes.

It also exposes a second embedded-admin module base for testing
`AdminSurface::EmbeddedCustom`:

- `GET /lenso/module/v1/embedded/manifest`
- `GET /lenso/module/v1/embedded/admin`

And a declarative custom admin module base for testing
`AdminSurface::DeclarativeCustom`:

- `GET /lenso/module/v1/declarative/manifest`
- `GET /lenso/module/v1/declarative/admin/contacts?limit=50&cursor=...`
- `GET /lenso/module/v1/declarative/admin/contacts/{id}`

Run it locally:

```sh
cargo run --locked -p remote-module-example
```

The server listens on `127.0.0.1:4100` by default. Override it with:

```sh
REMOTE_MODULE_ADDR=127.0.0.1:4101 cargo run --locked -p remote-module-example
```

Run the same fixture as a native gRPC remote module:

```sh
cargo run --locked -p remote-module-example -- --grpc
```

Connect it to the API in another shell:

```sh
REMOTE_MODULES=remote-crm=http://127.0.0.1:4100/lenso/module/v1 just api
```

Or connect the gRPC transport:

```sh
REMOTE_MODULES=remote-crm=grpc://127.0.0.1:4100 just api
```

The API loads the module manifest at startup. The HTTP transport also serves
schema-admin data through the normal `/admin/data/*` backend; the gRPC transport
currently covers manifest, runtime function, and event-handler calls.
The manifest also declares module-local HTTP route metadata for `/contacts`,
`/contacts/{id}`, and proxy fixture routes. The host preserves that metadata
under `/admin/data/modules` and exposes matched routes through:

```text
/modules/remote-crm/http/{*path}
```

Proxy calls are persisted in `platform.remote_http_proxy_calls` with
request/correlation/trace/span context. Runtime Console shows them in the
horizontal Remote Calls page, as `remote_proxy_call` nodes in Runtime Story
graph/timeline views, and as `source = "remote_proxy"` rows in Technical
Operations.

## Runtime Story Smoke Test

Use this flow when checking that the remote HTTP proxy is visible from the
Runtime Story perspective.

From the repo root, start Postgres and migrations, then run the remote module
fixture and API in separate shells:

```sh
just db-up
just migrate
cargo run --locked -p remote-module-example
REMOTE_MODULES=remote-crm=http://127.0.0.1:4100/lenso/module/v1,remote-crm-embedded=http://127.0.0.1:4100/lenso/module/v1/embedded,remote-crm-declarative=http://127.0.0.1:4100/lenso/module/v1/declarative just api
```

Seed and verify the remote story path:

```sh
just console-api-qa
```

`console-api-qa` creates a deterministic remote proxy call with
`correlation_id = corr_console_api_fixture`, then verifies Remote Calls, Runtime
Story nodes/timeline, Technical Operations, payloads, and logs.

To create only the fixture without running the full QA assertions:

```sh
just console-api-fixture
```

To run only the API smoke assertions against existing data:

```sh
just console-api-smoke
```

The host path after `/modules/remote-crm/http` is matched against the module
manifest route `/contacts/{id}`. A path such as `/contact_1` or a token missing
`remote_crm.contacts.read` will not hit this declared route.

In Runtime Console, verify:

- Remote Calls contains `corr_console_api_fixture`.
- Stories contains `corr_console_api_fixture` with a `Remote Call` timeline row.
- The row summary shows `ok / remote-crm / GET /contacts/{id} / status 200`.
- Selecting the remote call node shows request, trace, span, path params, and
  route details in the Inspector.
- Technical Operations includes a row with `source = remote_proxy`.

Then trigger a successful proxied contact fetch:

```sh
curl \
  -H "Authorization: Bearer dev-service:admin:remote_crm.contacts.read" \
  -H "x-request-id: req_demo_remote_story_1" \
  -H "x-correlation-id: corr_demo_remote_story_1" \
  http://localhost:3000/modules/remote-crm/http/contacts/contact_1
```

Trigger a failed remote response that is still recorded as a remote proxy call:

```sh
curl \
  -H "Authorization: Bearer dev-service:admin:remote_crm.contacts.read" \
  -H "x-request-id: req_demo_remote_story_2" \
  -H "x-correlation-id: corr_demo_remote_story_2" \
  http://localhost:3000/modules/remote-crm/http/proxy-fixtures/text
```

The failure request creates a failed `remote_proxy_call` node and keeps its
remote error details in Inspector and Technical Operations.

To load both the schema-admin module and the embedded iframe module:

```sh
REMOTE_MODULES=remote-crm=http://127.0.0.1:4100/lenso/module/v1,remote-crm-embedded=http://127.0.0.1:4100/lenso/module/v1/embedded,remote-crm-declarative=http://127.0.0.1:4100/lenso/module/v1/declarative just api
```

The embedded manifest points at the example's `/embedded/admin` page with an
origin allowlist for the current request host, so the Runtime Console can render
it in a sandboxed iframe without a host bridge.
The declarative manifest uses host-rendered `metric_strip`, `entity_table`, and
`entity_detail` sections backed by the same Contacts fallback schema. The table
and detail sections are read-only and use the declarative admin data endpoints
above; the fallback schema is not advertised as a generic schema-admin module.

Use `just console-api-qa` for the broader Remote Calls and Runtime Story QA
flow after the backend services are running.
