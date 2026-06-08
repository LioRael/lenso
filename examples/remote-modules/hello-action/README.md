# Hello Action Remote Module

This is the smallest release-demo remote module for Lenso. It exposes:

- a manifest at `http://127.0.0.1:4100/lenso/module/v1/manifest`;
- one HTTP route, `GET /hello/{name}`;
- one runtime function, `hello-action.say-hello.v1`;
- one schema-admin entity, `greetings`.

Run it from the repository root:

```sh
node examples/remote-modules/hello-action/src/server.mjs
```

Install it into a local Lenso checkout:

```sh
lenso module add http://127.0.0.1:4100/lenso/module/v1/manifest
lenso console-package apply-plan
```

For a non-interactive smoke:

```sh
just demo-release
```
