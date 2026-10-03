# One sentence to a source Plugin

> Add a local Rust `POST /greet` Plugin that trims a name, returns `Hello, <name>!`, rejects empty or overlong names, and prove it with affected tests and a real request; no publication or deployment.

The [greeting source](greeting-source/src/lib.rs) is the complete behavior. It
uses the existing `#[lenso::plugin]`, `#[endpoint]`, `#[post]`, `Json` and `Problem`
APIs. The source App builder supplies the existing Web Ingress and native Host.
There is no new route DSL, business Host or Capability contract to learn.

This is a source-checkout example, not a claim that the selected package cohort
is published. Its path dependencies intentionally use this repository. From
the repository root, with its configured Rust toolchain and Python 3:

```sh
cargo build --locked -p lenso-cli
target/debug/lenso app discover --root examples/agent-flow/greeting-source --json
cargo test --locked --manifest-path examples/agent-flow/greeting-source/Cargo.toml
target/debug/lenso app build --root examples/agent-flow/greeting-source
target/debug/lenso app check --root examples/agent-flow/greeting-source/dist
target/debug/lenso app show --root examples/agent-flow/greeting-source/dist
target/debug/lenso app start --from examples/agent-flow/greeting-source/dist
```

Use the actual listener address printed after readiness. In another terminal:

```sh
curl -i -H 'Content-Type: application/json' \
  -d '{"name":"  Lenso  "}' http://127.0.0.1:8080/greet
curl -i -H 'Content-Type: application/json' \
  -d '{"name":"   "}' http://127.0.0.1:8080/greet
```

Expect 200 with `{"message":"Hello, Lenso!"}` and 400 Problem Details with
`invalid_name`. Ctrl-C stops the Host. For automatic verification on a free
loopback port, run the [smoke](smoke.py):

```sh
python3 examples/agent-flow/smoke.py --cli target/debug/lenso
```

The smoke creates an empty temporary source App with the existing CLI, copies
the example behavior into it and builds a new distribution. It runs the affected
test and actual CLI discovery/build/check/show/configure, waits for the real listener,
then sends successful and rejected HTTP requests and stops its own Host. It
prints actual command exits and responses. It needs no credentials or external
service. Existing `dist` is never overwritten: for another manual build select
a new output with `app build --out <new-directory>`.

For a new project rather than this copyable sample, inspect `plugin new --help`
and use `lenso plugin new company.greeting --web`. Edit its generated source,
run its affected tests, then `lenso plugin dev` and call the printed route.
`--no-install` only defers dependency installation/checking; it is not execution
proof. Reuse current diagnostics if toolchain or registry setup fails.

The minimum concepts are:

| Concept | In this example |
| --- | --- |
| Plugin | `GreetingHttp` owns name validation and response behavior; ordinary helpers stay in its package |
| Instance | App-owned `example.agent-greeting/default`, selected by the single-Plugin package fallback |
| Config | None needed for this stateless behavior; typed Instance overrides live in `plugins/<id>/<instance>.toml` when needed |
| Dependency | Existing HTTP Endpoint role and Web Ingress; no new business collaboration contract |
| Target | Native for this slice; another target needs its own support/resource validation and lowering |

Local source needs neither `pack` nor a frozen release. Capability authoring,
durable state, Auth, money, migration, portable packaging and other languages
are added only when required by the request. Each future Instance keeps its own
configuration, bindings, resources, state and permission scope. Repository final
review and candidate CI still apply once at delivery. The
[agent guide](../../docs/agents/development-loop.md) describes risk-based checks
and the CLI/library/MCP relationship.
See [observed command feedback](feedback.md) for the executed path and newcomer
setup costs; the smoke prints fresh evidence on every run.
