use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use lenso_app_authoring::identity::validate_plugin_id_v1;

use super::{PluginNewArgs, PluginRuntimeArg, WASM_TARGET, run_bun, run_cargo};

pub(super) const LENSO_FRAMEWORK_REVISION: &str = "c2e77e3ccc4b7dabe2e1596f6c513641798330f8";

pub(super) fn create(args: PluginNewArgs) -> anyhow::Result<()> {
    validate_plugin_id_v1(&args.plugin_id)?;
    let base = args.repo_root.unwrap_or(env::current_dir()?);
    let target = args
        .dir
        .map_or_else(|| base.join(&args.plugin_id), |dir| base.join(dir));
    if target.exists() {
        bail!(
            "Plugin project directory already exists: {}",
            target.display()
        );
    }
    let files = if args.web {
        web_plugin_scaffold(&args.plugin_id)
    } else {
        match args.runtime {
            PluginRuntimeArg::Multi => multi_plugin_scaffold(&args.plugin_id),
            PluginRuntimeArg::Wasm => plugin_scaffold(&args.plugin_id),
            PluginRuntimeArg::Process => process_plugin_scaffold(&args.plugin_id),
            PluginRuntimeArg::Bun => bun_plugin_scaffold(&args.plugin_id),
        }
    };
    if args.dry_run {
        println!("Plugin dry run for {}:", target.display());
        for path in files.keys() {
            println!("  {}", path.display());
        }
        return Ok(());
    }
    fs::create_dir_all(&base)
        .with_context(|| format!("create Plugin project parent {}", base.display()))?;
    let staging = tempfile::Builder::new()
        .prefix(".lenso-plugin-new-")
        .tempdir_in(&base)
        .context("create Plugin scaffold staging directory")?;
    for (path, contents) in files {
        let destination = staging.path().join(path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, contents)?;
    }
    fs::rename(staging.path(), &target)
        .with_context(|| format!("publish Plugin scaffold {}", target.display()))?;
    if !args.no_install {
        if args.web {
            run_cargo(
                &target,
                &["generate-lockfile"],
                "generate Web Plugin lockfile",
            )?;
            run_cargo(&target, &["test", "--locked"], "test generated Web Plugin")?;
        } else if args.runtime == PluginRuntimeArg::Bun {
            run_bun(&target, &["install"], "install Bun Plugin dependencies")?;
            run_bun(&target, &["run", "check"], "check generated Bun Plugin")?;
        } else {
            run_cargo(&target, &["generate-lockfile"], "generate Plugin lockfile")?;
        }
        if !args.web
            && matches!(
                args.runtime,
                PluginRuntimeArg::Multi | PluginRuntimeArg::Wasm
            )
        {
            run_cargo(
                &target,
                &["check", "--locked", "--lib", "--target", WASM_TARGET],
                "check generated Wasm implementation",
            )?;
        }
        if !args.web
            && matches!(
                args.runtime,
                PluginRuntimeArg::Multi | PluginRuntimeArg::Process
            )
        {
            run_cargo(
                &target,
                &[
                    "check",
                    "--locked",
                    "--bin",
                    &args.plugin_id.replace('.', "-"),
                ],
                "check generated Process implementation",
            )?;
        }
    }
    println!("Created Plugin project at {}.", target.display());
    Ok(())
}

#[allow(clippy::too_many_lines)] // The generated, copyable source is kept in one visible template.
pub(super) fn web_plugin_scaffold(plugin_id: &str) -> BTreeMap<PathBuf, String> {
    let package_name = plugin_id.replace('.', "-");
    let manifest = format!(
        r#"[package]
name = "{package_name}"
version = "0.1.0"
edition = "2024"
publish = false

[package.metadata.lenso]
plugin-id = "{plugin_id}"
root-slot = "web"

[dependencies]
lenso = {{ version = "=0.5.25", git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}
lenso-capability-http-endpoint = {{ version = "0.3.4", git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}
serde = {{ version = "1", features = ["derive"] }}
schemars = "1.2"

[dev-dependencies]
bytes = "1"
futures = "0.3"
http = "1"
lenso-app-plan = {{ version = "=0.4.5", git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}
lenso-kernel = {{ version = "=0.3.11", git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}
lenso-test = {{ version = "=0.1.2", git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}
lenso-web-host = {{ version = "0.2.2", git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}

[patch.crates-io]
lenso = {{ git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}
lenso-app-plan = {{ git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}
lenso-kernel = {{ git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}
lenso-native-adapter = {{ git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}
lenso-test = {{ git = "https://github.com/LioRael/lenso", rev = "{LENSO_FRAMEWORK_REVISION}" }}

[workspace]
"#
    );
    let source = r#"use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

use lenso_capability_http_endpoint::{
    prelude::*,
    response::{Problem, StatusCode},
    JsonSchema,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
struct CreateGreeting {
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct SearchGreetings {
    term: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
struct Greeting {
    id: String,
    message: String,
}

#[lenso::plugin]
#[derive(Clone, Debug, Default)]
pub struct GreetingsHttp {
    next_id: Rc<Cell<u64>>,
    greetings: Rc<RefCell<BTreeMap<String, Greeting>>>,
}

/// Keeps this Plugin's generated factory linked into a native Host binary.
pub const fn link() {}

#[endpoint]
impl GreetingsHttp {
    #[post("greetings.create", "/greetings")]
    #[openapi({
        summary: "Create a greeting",
        responses: {
            "201": { description: "Greeting created" }
        }
    })]
    #[openapi_contract(
        success = 201,
        errors = [(400, "invalid_name")]
    )]
    async fn create(
        &self,
        Json(input): Json<CreateGreeting>,
    ) -> Result<(StatusCode, Json<Greeting>), Problem> {
        // A real Plugin normally awaits its business Capability here.
        std::future::ready(()).await;
        let name = input.name.trim();
        if name.is_empty() {
            return Err(Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid_name",
                "name must not be empty",
            ));
        }

        let sequence = self.next_id.get() + 1;
        self.next_id.set(sequence);
        let greeting = Greeting {
            id: format!("greeting-{sequence}"),
            message: format!("Hello, {name}!"),
        };
        self.greetings
            .borrow_mut()
            .insert(greeting.id.clone(), greeting.clone());
        Ok((StatusCode::CREATED, Json(greeting)))
    }

    #[query("greetings.search", "/greetings/search")]
    async fn search(
        &self,
        Json(input): Json<SearchGreetings>,
    ) -> Result<Json<Vec<Greeting>>, Problem> {
        std::future::ready(()).await;
        let greetings = self
            .greetings
            .borrow()
            .values()
            .filter(|greeting| greeting.message.contains(&input.term))
            .cloned()
            .collect();
        Ok(Json(greetings))
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;
    use lenso_capability_http_endpoint::testing::EndpointTest;

    use super::*;

    #[test]
    fn creates_and_queries_a_greeting_without_opening_a_socket() {
        block_on(async {
            let endpoint = EndpointTest::new(GreetingsHttp::default());
            let created = endpoint
                .request("greetings.create")
                .json(&CreateGreeting {
                    name: "Lenso".to_owned(),
                })
                .unwrap()
                .send()
                .await
                .unwrap();
            assert_eq!(created.status(), StatusCode::CREATED);

            let found = endpoint
                .request("greetings.search")
                .json(&SearchGreetings {
                    term: "Lenso".to_owned(),
                })
                .unwrap()
                .send()
                .await
                .unwrap();
            assert_eq!(found.status(), StatusCode::OK);
            assert_eq!(found.json::<Vec<Greeting>>().unwrap().len(), 1);
        });
    }

    #[test]
    fn turns_business_rejections_into_problem_responses() {
        let response = block_on(async {
            EndpointTest::new(GreetingsHttp::default())
                .request("greetings.create")
                .json(&CreateGreeting {
                    name: String::new(),
                })
                .unwrap()
                .send()
                .await
                .unwrap()
        });

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.header("content-type"),
            Some("application/problem+json; charset=utf-8")
        );
    }
}
"#
    .to_owned();
    let simulated_test = format!(
        concat!(
            "//! Socket-free Web contract test through the real event Ingress.\n",
            "//! `SimulatedWebHost` deliberately does not call the handler directly.\n\n",
            "use std::time::Duration;\n\n",
            "use bytes::Bytes;\n",
            "use http::Request;\n",
            "use lenso_kernel::ShutdownOutcome;\n",
            "use lenso_test::TestApp;\n",
            "use lenso_web_host::NativeWebHost;\n\n",
            "use {crate_name}::GreetingsHttp;\n\n",
            "#[test]\n",
            "fn creates_a_greeting_through_real_ingress_without_a_tcp_listener() {{\n",
            "    let prepared = NativeWebHost::new()\n",
            "        .plugin::<GreetingsHttp>()\n",
            "        .prepare_simulated()\n",
            "        .expect(\"a native Web Plugin can prepare real event ingress\");\n",
            "    let (plan, registry, web) = prepared.into_parts();\n",
            "    let app = TestApp::builder(plan).with_registry(registry).start().unwrap();\n\n",
            "    let response = app.run(web.request(\n",
            "        Request::builder().method(\"POST\").uri(\"/greetings\")\n",
            "            .header(\"content-type\", \"application/json\")\n",
            "            .body(Bytes::from_static(br#\"{{\"name\":\"Lenso\"}}\"#)).unwrap(),\n",
            "    )).unwrap();\n",
            "    assert_eq!(response.status(), 201);\n",
            "    assert!(response.body().starts_with(br#\"{{\"id\":\"greeting-1\"\"#));\n\n",
            "    assert_eq!(app.shutdown(Duration::from_secs(1)), ShutdownOutcome::Clean);\n",
            "}}\n",
        ),
        crate_name = package_name.replace('-', "_"),
    );
    let golden_path = concat!(
        "# Web Plugin golden path\n\n",
        "The generated `src/lib.rs` is the normal starting point: typed JSON, a structured `Problem`, an opt-in strict OpenAPI operation, a small unit test, and `lenso plugin dev` for a real loopback request. It deliberately does not require knowing about generations, drivers, adapter catalogs, or factories.\n\n",
        "## Add an authenticated business endpoint\n\n",
        "Authentication at the HTTP edge is a typed Capability dependency, not a middleware global. Add the product-owned Auth and business Capability crates, then make their generated clients explicit dependencies of the endpoint Plugin. The native authoring shape is intentionally small:\n\n",
        "```rust,ignore\n",
        "#[lenso::plugin]\n",
        "#[derive(Debug)]\n",
        "struct OrdersHttp {\n",
        "    #[dependency(id = \"auth\")]\n",
        "    auth: lenso_capability_auth::AuthClient,\n",
        "    #[dependency(id = \"orders\")]\n",
        "    orders: company_orders::OrdersClient,\n",
        "}\n",
        "```\n\n",
        "The App plan selects the concrete Auth and Orders providers and binds those named requirements. It is the only place that chooses providers. Do not look up a database, an Auth provider, or another Plugin from a global service.\n\n",
        "For a request actor, use `lenso-http-auth`'s `AuthenticatedHttpActor` and `extract_authenticated_actor`; it turns the already bound Auth client into a typed edge actor. The business Capability still verifies authorization and resource ownership.\n\n",
        "## Keep public OpenAPI honest\n\n",
        "`create` is marked with `#[openapi_contract]`. Its request body, success value, and stable `invalid_name` problem code are derived from the same typed handler values. Select and bind the optional OpenAPI Plugin only when this route is a public API; activation then rejects a document that drifts from the handler. Private routes may omit the attribute entirely.\n\n",
        "## Exercise the real Web path locally\n\n",
        "`tests/simulated_web.rs` starts a `TestApp` with the exact Host-generated plan and registry, then sends a request through `SimulatedWebHost`. It does not open a socket and does not call a handler directly. The generated manifest Git-pins `lenso-web-host@0.2.2`, Endpoint `0.3.4`, `lenso@0.5.25`, `lenso-test@0.1.2`, App Plan `0.4.5`, and Kernel `0.3.11`; its root patch makes the Host, Plugin, adapter, and TestApp share those exact type identities. Run it with `cargo test --locked`. Do not replace those pins with independent registry ranges until the cohort release validation says they are published together.\n\n",
        "## Add a stream deliberately\n\n",
        "Buffered HTTP and a long-lived stream are separate public interactions. When a route needs backpressure or a persistent session, add the dedicated `lenso-capability-http-stream-endpoint` contract and test it through `SimulatedWebHost::open_stream`. Keep its route identifier and typed protocol next to the business Capability it invokes; do not turn a buffered `#[endpoint]` handler into an ad-hoc socket loop. The same surface also exposes `open_websocket` when a bidirectional protocol is the actual requirement.\n",
    )
    .to_owned();
    let readme = format!(
        "# {plugin_id}\n\nLinked native Rust Web Plugin using `#[lenso::plugin]` and `#[endpoint]`.\n\n```sh\ncargo test --locked\nlenso plugin dev\n```\n\nThe generated tests invoke typed Endpoint operations and the real event Ingress without opening a socket. `lenso plugin dev` builds a temporary native Host, mounts this Plugin through the `web` root slot, starts a loopback Web Ingress listener, and prints the real HTTP routes. Add `--watch` to rebuild and restart after source changes.\n\nSee [the Web golden path](WEB_GOLDEN_PATH.md) to add an authenticated business Capability, strict public OpenAPI, a simulated Host test, or a streaming endpoint without making the basic route depend on Runtime internals.\n"
    );

    BTreeMap::from([
        (PathBuf::from("Cargo.toml"), manifest),
        (PathBuf::from("src/lib.rs"), source),
        (PathBuf::from("tests/simulated_web.rs"), simulated_test),
        (PathBuf::from("WEB_GOLDEN_PATH.md"), golden_path),
        (PathBuf::from("README.md"), readme),
    ])
}

pub(super) fn bun_plugin_scaffold(plugin_id: &str) -> BTreeMap<PathBuf, String> {
    let package_name = plugin_id.replace('.', "-");
    BTreeMap::from([
        (
            PathBuf::from("package.json"),
            bun_package_manifest(plugin_id, &package_name),
        ),
        (
            PathBuf::from("tsconfig.json"),
            r#"{
  "compilerOptions": {
    "lib": ["ES2023"],
    "module": "Preserve",
    "moduleResolution": "bundler",
    "noEmit": true,
    "strict": true,
    "allowImportingTsExtensions": true,
    "types": ["bun"]
  },
  "include": ["src/**/*.ts"]
}
"#
            .to_owned(),
        ),
        (PathBuf::from("src/plugin.ts"), bun_author_source(plugin_id)),
        (
            PathBuf::from("README.md"),
            format!(
                "# {plugin_id}\n\nTyped Bun Plugin using the generic Lenso Plugin SDK and Agent-owned Tool declarations. Edit `src/plugin.ts`; the CLI compiles declarations and runtime bindings.\n\n```sh\nlenso plugin check\nlenso plugin dev --operation execute --request-json '{{\"name\":\"{plugin_id}\",\"arguments_json\":\"{{\\\"text\\\":\\\"hello\\\"}}\"}}'\nlenso plugin dev --watch\nlenso plugin pack\n```\n"
            ),
        ),
    ])
}

fn bun_package_manifest(plugin_id: &str, package_name: &str) -> String {
    format!(
        r#"{{
  "name": "{package_name}",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {{
    "check": "tsc --noEmit"
  }},
  "dependencies": {{
    "@lenso/agent-tool-sdk": "0.1.0",
    "@lenso/bun-plugin": "0.2.2"
  }},
  "devDependencies": {{
    "@types/bun": "1.4.0",
    "typescript": "7.0.2"
  }},
  "lenso": {{
    "pluginId": "{plugin_id}",
    "rootSlot": "tool-providers",
    "runtime": "bun"
  }}
}}
"#
    )
}

fn bun_author_source(plugin_id: &str) -> String {
    format!(
        r#"import {{ definePlugin }} from "@lenso/bun-plugin";
import {{ tool, tools }} from "@lenso/agent-tool-sdk";
import * as schema from "@lenso/agent-tool-sdk/schema";

export default definePlugin({{
  providers: [
    tools([
      tool(
        {{
          name: "{plugin_id}",
          description: "Uppercase one UTF-8 string.",
          input: schema.object({{ text: schema.string() }}),
          output: schema.string(),
          execution: "parallel_safe",
        }},
        ({{ text }}) => ({{ ok: true, value: text.toUpperCase() }}),
      ),
    ]),
  ],
}});
"#
    )
}

pub(super) fn multi_plugin_scaffold(plugin_id: &str) -> BTreeMap<PathBuf, String> {
    let mut files = plugin_scaffold(plugin_id);
    let manifest = files
        .get_mut(Path::new("Cargo.toml"))
        .expect("Wasm scaffold has a manifest");
    *manifest = manifest.replace("runtime = \"wasm\"", "outputs = [\"wasm\", \"process\"]");
    files.insert(
        PathBuf::from("src/main.rs"),
        "// Cargo Process entrypoint; the SDK supplies main and protocol lowering.\ninclude!(\"lib.rs\");\n"
            .to_owned(),
    );
    files.insert(
        PathBuf::from("README.md"),
        format!(
            "# {plugin_id}\n\nOne ordinary Rust Plugin source with portable Wasm and trusted Process outputs. `lenso plugin pack` builds both implementations into one release.\n"
        ),
    );
    files
}

pub(super) fn plugin_scaffold(plugin_id: &str) -> BTreeMap<PathBuf, String> {
    let package_name = plugin_id.replace('.', "-");
    BTreeMap::from([
        (
            PathBuf::from("Cargo.toml"),
            format!(
                r#"[package]
name = "{package_name}"
version = "0.1.0"
edition = "2024"
publish = false

[package.metadata.lenso]
plugin-id = "{plugin_id}"
root-slot = "tool-providers"

[package.metadata.lenso-cli]
runtime = "wasm"

[lib]
crate-type = ["cdylib"]

[dependencies]
lenso = {{ package = "lenso-plugin-sdk", version = "0.4.1" }}
lenso-agent-tool-sdk = "0.3.0"
schemars = "1"
serde = {{ version = "1", features = ["derive"] }}

[workspace]
"#
            ),
        ),
        (
            PathBuf::from("src/lib.rs"),
            format!(
                r#"use lenso_agent_tool_sdk::prelude::*;
use schemars::JsonSchema;

#[derive(Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Arguments {{
    #[schemars(length(max = 4096))]
    text: String,
}}

#[lenso::plugin]
#[derive(Clone, Copy, Debug, Default)]
struct Plugin {{}}

#[lenso_agent_tool_sdk::tool_provider]
impl Plugin {{
    #[tool(
        name = "{plugin_id}",
        description = "Process one UTF-8 string.",
        execution = "parallel_safe"
    )]
    fn execute(arguments: Arguments) -> Result<ExecuteResponse, ExecuteError> {{
        if arguments.text.is_empty() {{
            return Err(ExecuteError::InvalidArguments);
        }}
        Ok(ExecuteResponse {{
            content: arguments.text,
            content_blocks: None,
            content_type: ContentType::Text,
            metadata_json: "{{}}"
                .try_into()
                .expect("static Tool metadata must be valid JSON"),
        }})
    }}
}}
"#
            ),
        ),
        (
            PathBuf::from("README.md"),
            format!(
                "# {plugin_id}\n\nOrdinary Rust Plugin for Lenso Agent, packaged as an isolated Wasm Component. The SDK owns the execution bridge.\n\n```sh\nlenso plugin check\nlenso plugin dev --operation execute --request-json '{{\"name\":\"{plugin_id}\",\"arguments_json\":\"{{\\\"text\\\":\\\"hello\\\"}}\"}}'\nlenso plugin pack\n```\n\nCreate another project with `lenso plugin new <id>`.\n"
            ),
        ),
    ])
}

pub(super) fn process_plugin_scaffold(plugin_id: &str) -> BTreeMap<PathBuf, String> {
    let mut files = plugin_scaffold(plugin_id);
    let manifest = files
        .get_mut(Path::new("Cargo.toml"))
        .expect("Plugin scaffold has a manifest");
    *manifest = manifest.replace("runtime = \"wasm\"", "runtime = \"process\"");
    files.insert(
        PathBuf::from("src/main.rs"),
        "// Cargo Process entrypoint; the SDK supplies main and protocol lowering.\ninclude!(\"lib.rs\");\n"
            .to_owned(),
    );
    files.insert(
        PathBuf::from("README.md"),
        format!(
            "# {plugin_id}\n\nOrdinary Rust source compiled as a trusted native Process Plugin. The SDK owns the protocol bridge and runtime descriptor. Process Plugins are not sandboxed, so install only trusted bundles.\n\n```sh\nlenso plugin check\nlenso plugin dev --operation execute --request-json '{{\"name\":\"{plugin_id}\",\"arguments_json\":\"{{\\\"text\\\":\\\"hello\\\"}}\"}}'\nlenso plugin pack\n```\n"
        ),
    );
    files
}
