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

To load both the schema-admin module and the embedded iframe module:

```sh
REMOTE_MODULES=remote-crm=http://127.0.0.1:4100/lenso/module/v1,remote-crm-embedded=http://127.0.0.1:4100/lenso/module/v1/embedded just api
```

The embedded manifest points at the example's `/embedded/admin` page with an
origin allowlist for the current request host, so the Runtime Console can render
it in a sandboxed iframe without a host bridge.

For a one-command local Console demo from the repo root:

```sh
just embedded-admin-demo
```
