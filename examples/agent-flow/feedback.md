# Observed command feedback

Executed on Linux x86_64 on 2026-10-03, Rust/Cargo 1.94.0, from source base
`66907ae817cff233ac47d179ad1e74bb82f7b9d2`. The example uses checkout dependencies,
not registry-install evidence. Commands below used the source-built `lenso`
0.7.1; the smoke replaced paths only in its temporary copy and selected a free
loopback port with the existing Ingress configuration command.

| Actual command/path | Observed feedback |
| --- | --- |
| `cargo build --locked -p lenso-cli` | Exit 0; cold CLI build completed in 4m 08s |
| `lenso app create SOURCE --runtime empty` | Exit 0; created an empty source App before copying the sample behavior |
| `lenso app discover --root SOURCE --json` | Exit 0; one `example.agent-greeting@0.1.0` App-owned `native-linked` candidate, `source_metadata_only` |
| `cargo test --locked --manifest-path SOURCE/Cargo.toml` | Exit 0; affected Endpoint test passed for trimmed, empty and overlong names |
| `lenso app build --root SOURCE --out NEW_DIST` | Exit 0; assembled two Plugin Instances; initial release Host compilation completed in 1m 18s |
| `lenso plugins configure lenso.web-ingress default --root NEW_DIST --file ingress.toml` | Exit 0; `bind_address = "127.0.0.1:0"` accepted |
| `lenso app check --root NEW_DIST` | Exit 0; `App is valid: 2 Plugin Instance(s), 1 Capability binding(s).` |
| `lenso app show --root NEW_DIST` | Exit 0; business Plugin and Ingress with `lenso.http.endpoint@1` binding |
| `lenso app start --from NEW_DIST` | Real `Listening on http://127.0.0.1:<port>/` after readiness |
| `POST /greet`, `{"name":"  Lenso  "}` | 200, `{"message":"Hello, Lenso!"}` |
| `POST /greet`, `{"name":"   "}` | 400 Problem Details, `code=invalid_name` |
| `POST /greet`, malformed JSON | 400 Problem Details, `code=invalid_json_body` |
| Stop smoke-owned Host with Ctrl-C/SIGINT | Exit 0; `source Plugin loop passed` |

The final warm-cache smoke rebuilt the temporary source App in 10.02s;
check/show/configure each completed in about 0.01s. These timings describe this
run's cache state, not a performance guarantee or a skill-imposed delay.

The current generated Host emitted two unused-variable warnings; they did not
change exit status or HTTP behavior. No framework patch was needed to author
the route. Re-run `smoke.py` for current evidence; timing depends on caches and
machine resources.

Newcomer observations:

- This environment initially lacked Rust commands. Installing the pinned
  toolchain and cold compilation were setup/build costs, not mandatory skill
  stages. `--no-install` would defer this work rather than prove execution.
- Discovery reports source metadata; only build/check and a real request prove
  runnable behavior. The sample needs one business Plugin plus existing
  Ingress, not a new Capability contract or business Host.
- Local source works without a `.lenso-plugin` archive, signing or freezing.
  Generated outputs remain build artifacts. A second build needs a new output
  directory rather than overwriting `dist`.
- `npx skills add ./skills --list` found six canonical authoring Skills.
  Repository-root discovery also found the existing local `lenso-land` Skill.
  A sandboxed npm cache needed a writable temporary path; no permissions were
  changed. The Python pack validator and all five validator tests passed.

This is executable CLI/Plugin evidence, not an independent model-evaluation
transcript, published package qualification, target conformance for Workers or
portable runtimes, or candidate CI evidence.
