# Remote Module Example

Small out-of-process module fixture for the Lenso Remote module protocol.

It exposes a read-only CRM-style Contacts module:

- `GET /lenso/module/v1/manifest`
- `GET /lenso/module/v1/admin/contacts?limit=50&cursor=...`
- `GET /lenso/module/v1/admin/contacts/{id}`

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

Connect it to the API in another shell:

```sh
REMOTE_MODULES=remote-crm=http://127.0.0.1:4100/lenso/module/v1 just api
```

The API loads the module manifest at startup and serves its schema-admin data
through the normal `/admin/data/*` backend.
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

Start the remote module fixture:

```sh
cargo run --locked -p remote-module-example
```

In another shell, start local infrastructure and run migrations:

```sh
just db-up
just migrate
```

Start the API with the remote module configured:

```sh
REMOTE_MODULES=remote-crm=http://127.0.0.1:4100/lenso/module/v1 just api
```

Start Runtime Console against that API:

```sh
just console-api
```

Trigger a successful proxied contact fetch:

```sh
curl \
  -H "Authorization: Bearer dev-service:admin:remote_crm.contacts.read" \
  -H "x-request-id: req_demo_remote_story_1" \
  -H "x-correlation-id: corr_demo_remote_story_1" \
  http://localhost:3000/modules/remote-crm/http/contacts/contact_1
```

The host path after `/modules/remote-crm/http` is matched against the module
manifest route `/contacts/{id}`. A path such as `/contact_1` or a token missing
`remote_crm.contacts.read` will not hit this declared route.

Trigger a failed remote response that is still recorded as a remote proxy call:

```sh
curl \
  -H "Authorization: Bearer dev-service:admin:remote_crm.contacts.read" \
  -H "x-request-id: req_demo_remote_story_2" \
  -H "x-correlation-id: corr_demo_remote_story_2" \
  http://localhost:3000/modules/remote-crm/http/proxy-fixtures/text
```

In Runtime Console, verify:

- Stories contains `corr_demo_remote_story_1` with a `Remote Call` timeline row.
- The row summary shows `ok / remote-crm / GET /contacts/{id} / status 200`.
- Selecting the remote call node shows a `remote proxy` block in Inspector
  overview with request, trace, span, path params, and route details.
- Technical Operations includes a row with `source = remote_proxy`.
- Remote Calls can be filtered by `correlation_id = corr_demo_remote_story_1`.
- The failure request creates a failed `remote_proxy_call` node and keeps its
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

For a one-command local Console demo from the repo root:

```sh
just embedded-admin-demo
```
