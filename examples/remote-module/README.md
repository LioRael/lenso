# Remote Module Example

Small out-of-process module fixture for the Lenso Remote module protocol.

It exposes a read-only CRM-style Contacts module:

- `GET /lenso/module/v1/manifest`
- `GET /lenso/module/v1/admin/contacts?limit=50&cursor=...`
- `GET /lenso/module/v1/admin/contacts/{id}`

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
