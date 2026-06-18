# lenso-host

`lenso-host` is a compatibility crate for Lenso applications generated before
the host facade moved to `lenso::host`.

Prefer the `lenso` crate with the `host` feature:

```toml
lenso = { git = "https://github.com/LioRael/lenso", rev = "<commit>", features = ["host"] }
```

The facade intentionally stays small:

- `HostBuilder`, `HostComposition`, and `HostLinkedModule` for host-owned linked modules;
- `run_api_from_env_with_composition`, `run_worker_from_env_with_composition`, and `run_migrations_from_env_with_composition`;
- `Migration` and `ModuleManifest` re-exports for linked module metadata;
- `lenso::host::http` helpers for linked Axum routes and OpenAPI registration.

Application SQL, repositories, auth/session policy, CRUD shape, and Runtime
Console UI stay in the host application or module code.

Existing hosts can keep `lenso-host` while migrating imports to `lenso::host`.
