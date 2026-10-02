# Todo HTTP

A small Lenso endpoint Plugin for ordinary Todo CRUD. It runs through
`NativeWebHost` and the real Web Ingress listener. One Plugin Instance owns an
in-memory store; stopping the process discards all todos.

This is a **source-checkout example**. It is built against this repository's
workspace packages. The Todo routes were written after the public-entry
experiment entered its diagnostic phase, adapting the greetings pattern.
`@lenso/cli@0.17.4` does not expose a CRUD template selector.

From the repository root, with the repository's Rust toolchain and Python 3:

```sh
cargo run --locked -p lenso-onboarding-todo-http -- --bind 127.0.0.1:8080
```

The server prints its address only after readiness. Press Ctrl-C to shut down.
Use `--bind 127.0.0.1:0` to let the operating system choose a free local port.

| Method | Path | Body | Success |
| --- | --- | --- | --- |
| POST | `/todos` | `{"title":"Read the docs"}` | 201 with Todo |
| GET | `/todos` | — | 200 with Todo array |
| GET | `/todos/{todo_id}` | — | 200 with Todo |
| PUT | `/todos/{todo_id}` | `{"title":"Read the docs","completed":true}` | 200 with replaced Todo |
| DELETE | `/todos/{todo_id}` | — | 204 with empty body |

Send JSON with `Content-Type: application/json`. New todos have `completed=false`.
Titles are trimmed and must contain 1–200 Unicode scalar values. PUT replaces
both fields. Unknown fields and malformed bodies are rejected. Invalid titles
return 400; missing todos return 404 with a `todo_not_found` Problem Details code.

```sh
curl -i http://127.0.0.1:8080/todos \
  -H 'Content-Type: application/json' -d '{"title":"Read the docs"}'
curl http://127.0.0.1:8080/todos/todo-1
curl -X PUT http://127.0.0.1:8080/todos/todo-1 \
  -H 'Content-Type: application/json' \
  -d '{"title":"Read the docs","completed":true}'
curl -i -X DELETE http://127.0.0.1:8080/todos/todo-1
```

Build and run the automatic socket smoke from the repository root:

```sh
cargo build --locked -p lenso-onboarding-todo-http
python3 examples/onboarding/todo-http/smoke.py
```

It starts the server on a free loopback port,
checks all five operations, validation and malformed input, 404/405/415
responses, unchanged state after rejected mutations, graceful shutdown, and
fresh state after restart. It uses only Python's standard library and performs
no external requests. Pass a binary path as the first argument when using a
custom target directory. `bash examples/onboarding/todo-http/smoke.sh` also
builds before testing; that convenience wrapper respects `CARGO_TARGET_DIR`
and defaults `CARGO_BUILD_JOBS` to 2.

The deletion boundary is `TodoHttp`: omitting `.plugin::<TodoHttp>()` from the
Host removes its routes and state. The Host owns listening and shutdown; the
endpoint owns input validation and Todo behavior. The example has no durable
storage, authentication, or cross-Plugin dependency.
