# Three small applications

These examples show ordinary application behavior through Lenso Plugins and a
real native Host. They use this checkout's workspace dependencies and lockfile.
They are source examples, not published templates or registry qualification.
Rust 1.94.0, Python 3.11 or later, and a Linux or macOS shell are required.

| Example | Behavior | Boundary exercised |
| --- | --- | --- |
| [Todo HTTP](todo-http/README.md) | Create, list, read, update, delete tasks | Typed HTTP routes, input errors, Instance state |
| [Background jobs](background-jobs/README.md) | Submit work and inspect local notifications | Readiness, managed tasks, cancellation and shutdown |
| [Metadata pipeline](metadata-pipeline/README.md) | Process synthetic file metadata | Two Plugins, a typed Capability and missing-provider rejection |

No example uploads a file, sends an external notification, or requires real
credentials. Listeners bind to loopback on an automatically chosen port. By
default, state is held in memory and resets at restart. Background jobs also
offer an optional single-App snapshot for orderly restart and recovery; see its
README for the limits. Multi-writer storage, durable external queues and
power-loss recovery remain outside these examples.

## Run all three

From the repository root:

```sh
rustc --version
cargo --version
git rev-parse HEAD
python3 examples/onboarding/smoke.py
```

The runner uses two Cargo jobs and allows up to ten minutes for the shared
build, followed by up to ninety seconds per smoke. It terminates timed-out
process groups and returns nonzero on failure. Each smoke starts its own Host,
asserts real HTTP responses, and shuts it down. To rerun behavior after a
successful build, use `python3 examples/onboarding/smoke.py --skip-build`.
The individual READMEs explain interactive startup and their deletion checks.

The runner's cleanup regression covers both timeout and Ctrl-C, including a
descendant that ignores termination:

```sh
python3 -B -m unittest discover -s examples/onboarding -p test_smoke.py
```

## Choose the right starting point

For a generated App, follow the official
[local Web App tutorial](https://lenso.dev/docs/core/app-quickstart/) using the
matching source-built CLI's absolute path. The standalone
[Plugin quickstart](https://lenso.dev/docs/core/quickstart/) is a separate path.
The npm wrapper version and the native `lenso --version` identify different
artifacts; record both when reporting a problem.

Keep generated exact dependency pins unchanged when testing published packages.
An unavailable package is a publication blocker. A registry DNS or proxy error
only proves that registry access failed. If the CLI fails but the same Cargo
command works directly, include both command results and proxy environment
variable **names**, without their values. These source examples do not turn
either kind of failure into a registry pass.

For a new application, give a coding agent one observable behavior from the
table, the matching checkout, and the smoke command. Require it to report its
source revision, exact commands, exit codes, real success and failure responses,
and any source-only dependencies. Do not count a successful build as a tested
application or claim a published package was used after adding a path override.
