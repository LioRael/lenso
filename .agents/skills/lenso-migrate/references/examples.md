# Paired native catalog example

The complete [TypeScript](../assets/catalog.ts) and [Rust](../assets/catalog.rs) programs use public Core APIs. They create two same-type catalogs whose `shared` key differs, inject each exact instance into its query plugin, call ordinary methods and close owned resources. The Rust outer query admits work through WorkGate; its dependency does not reenter. TS explicitly defines retained-service closure behavior; it does not pretend to have Rust admission/drain.

Both languages return `North Ada`, `South Grace` and close every instance exactly once, with each query closed before its own provider. Independent nodes can have different valid topological orders: do not require identical cross-language cleanup traces. `--fail` fails the second query setup after cleanup registration and verifies the same dependency/once-only rollback invariant. The examples retain operation and shutdown errors together. Their assertions are local example acceptance, not a new framework test suite.

Use prepared source checkouts. TS Core must have its normal built `dist` and dependencies; Rust uses its real path dependency, with a separate temporary consumer Cargo.lock. Run from the skill directory, supplying trusted absolute roots:

```sh
python3 scripts/verify-native.py   --ts-root /absolute/path/to/lenso   --rust-root /absolute/path/to/lenso-rust   --output-dir /tmp/lenso-native-proof
```

The output directory must be new/empty. `--bun` and `--cargo` select installed executables when shims cannot run; `--offline` restricts Cargo to its cache. The helper typechecks TS, compiles Rust, runs each normal/rollback case and compares values and released instances while retaining each language’s actual cleanup trace. It writes only its owned directory and normal tool caches, never the source repositories or their locks. Without `--output-dir` it removes its temporary artifacts.

Build TS Core with its manifest's existing build command after frozen preparation. If a newer Runtime candidate changes dependencies so the root frozen install rejects, use the pinned SDK preparation or report the integration lock gap; do not rewrite the root lock for this example. These programs require no Runtime SDK, kernel, ports, JSON wire contract or installed global skill.

Limits: this small acceptance does not prove concurrent request drain, panic/cancellation behavior, storage persistence, Auth or wire compatibility. Use the existing focused tests for a change in those boundaries rather than expanding this helper into a benchmark.
