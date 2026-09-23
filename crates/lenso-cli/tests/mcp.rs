//! Exercise the public MCP transport, not only the underlying facts function.
use std::{
    fs,
    io::{BufRead as _, BufReader, Write as _},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn stdio_exposes_bounded_read_only_app_facts() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("lenso.toml"),
        "not valid toml = SECRET_MARKER",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["mcp", "--root"])
        .arg(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project_facts","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"project_explain","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"linked_catalog","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"linked_document","arguments":{"plugin_id":"example.web","version":"0.4.5","document_id":"readme","revision":"1"}}}"#,
        r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"project_facts","arguments":{"section":"diagnostics","limit":1}}}"#,
        r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"project_facts","arguments":{"section":"diagnostics","offset":1,"limit":1}}}"#,
        r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"project_facts","arguments":{"offset":1}}}"#,
        r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"project_facts","arguments":{"section":"diagnostics","limit":0}}}"#,
        r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"project_check","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"project_build","arguments":{"request_id":"without-owner-authorization"}}}"#,
        r#"{"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"project_change_apply","arguments":{"proposal_digest":"sha256:unknown","request_id":"without-owner-authorization"}}}"#,
        r#"{"jsonrpc":"2.0","id":14,"method":"tools/call","params":{"name":"project_run","arguments":{"request_id":"without-owner-authorization"}}}"#,
    ].join("\n");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{input}\n").as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let frames = String::from_utf8(output.stdout).unwrap();
    assert!(!frames.contains("SECRET_MARKER"));
    let responses = frames
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 14, "{frames}");
    let by_id = responses
        .iter()
        .map(|response| (response["id"].as_u64().unwrap(), response))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(by_id.len(), 14);
    assert_tools(by_id[&2]);
    let facts: serde_json::Value =
        serde_json::from_str(by_id[&3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(facts["kind"], "lenso.app-facts");
    assert_eq!(facts["schema_version"], 3);
    assert_eq!(facts["status"], "invalid");
    assert_eq!(facts["runtime"]["status"], "not_observed");
    let first_page: serde_json::Value =
        serde_json::from_str(by_id[&7]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(first_page["kind"], "lenso.app-facts-page");
    assert_eq!(first_page["section"], "diagnostics");
    assert_eq!(first_page["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        first_page["total"],
        facts["diagnostics"].as_array().unwrap().len()
    );
    let second_page: serde_json::Value =
        serde_json::from_str(by_id[&8]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(second_page["offset"], 1);
    assert_eq!(second_page["items"].as_array().unwrap().len(), 1);
    assert_ne!(first_page["items"][0], second_page["items"][0]);
    for id in [4, 5, 6, 9, 10, 11, 12, 13, 14] {
        assert!(by_id[&id]["error"].is_object(), "response {id}");
    }
    assert!(!root.path().join("dist").exists());
}

fn assert_tools(list: &serde_json::Value) {
    let tools = list["result"]["tools"].as_array().unwrap();
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        names,
        [
            "linked_catalog",
            "linked_document",
            "project_check",
            "project_build",
            "project_build_cancel",
            "project_build_status",
            "project_change_apply",
            "project_change_preview",
            "project_explain",
            "project_facts",
            "project_run",
            "project_run_status",
            "project_run_stop",
            "project_selection_preview",
        ]
        .into()
    );
}

#[test]
fn stdio_explanation_matches_app_explain_json() {
    let temp = tempfile::tempdir().unwrap();
    let app = temp.path().join("source");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&app)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let root = app.join("dist");
    let explained = Command::new(cli)
        .args(["app", "explain", "--root"])
        .arg(&root)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        explained.status.success(),
        "{}",
        String::from_utf8_lossy(&explained.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&explained.stdout).unwrap();
    let checked = Command::new(cli)
        .args(["app", "check", "--root"])
        .arg(&root)
        .arg("--json")
        .output()
        .unwrap();
    assert!(checked.status.success());
    let expected_check: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();

    let mut child = Command::new(cli)
        .args(["mcp", "--root"])
        .arg(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"project_explain","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project_check","arguments":{}}}"#,
    ].join("\n");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{input}\n").as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 3);
    let explain_response = responses
        .iter()
        .find(|response| response["id"] == 2)
        .unwrap();
    let actual: serde_json::Value = serde_json::from_str(
        explain_response["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual, expected);
    let check_response = responses
        .iter()
        .find(|response| response["id"] == 3)
        .unwrap();
    let actual_check: serde_json::Value = serde_json::from_str(
        check_response["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual_check, expected_check);
}

#[test]
fn stdio_authorized_build_reports_the_same_app_check() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&root)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let mut child = Command::new(cli)
        .args(["mcp", "--root"])
        .arg(&root)
        .arg("--allow-build")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let call = |stdin: &mut std::process::ChildStdin,
                stdout: &mut BufReader<std::process::ChildStdout>,
                request: serde_json::Value|
     -> serde_json::Value {
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    let initialized = call(
        &mut stdin,
        &mut stdout,
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    assert_eq!(initialized["id"], 1);
    writeln!(
        stdin,
        "{}",
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    let start = call(
        &mut stdin,
        &mut stdout,
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"project_build","arguments":{"request_id":"mcp-build-proof"}}}),
    );
    assert!(start["error"].is_null(), "{start}");
    let status: serde_json::Value =
        serde_json::from_str(start["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(status["state"], "running");
    let deadline = Instant::now() + Duration::from_secs(120);
    let final_status = loop {
        let response = call(
            &mut stdin,
            &mut stdout,
            serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project_build_status","arguments":{"request_id":"mcp-build-proof"}}}),
        );
        assert!(response["error"].is_null(), "{response}");
        let status: serde_json::Value =
            serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        if status["state"] != "running" {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "MCP build did not finish: {status}"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(final_status["state"], "succeeded", "{final_status}");
    let checked = Command::new(cli)
        .args(["app", "check", "--json", "--root"])
        .arg(root.join("dist"))
        .output()
        .unwrap();
    assert!(checked.status.success());
    let expected: serde_json::Value = serde_json::from_slice(&checked.stdout).unwrap();
    assert_eq!(final_status["check"], expected);
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn stdio_configuration_preview_and_apply_use_plugin_root_authority() {
    use lenso_app_plan::authoring::{
        HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
    };

    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".lenso")).unwrap();
    let host = HostCatalog::new(
        [HostSlot::one("agent")],
        [HostPluginRelease::new(
            PluginDescriptor::new("example.agent", "1.0.0", "agent").with_configuration_schema(
                serde_json::json!({
                    "type":"object",
                    "properties":{"greeting":{"type":"string"}},
                    "additionalProperties":false
                }),
            ),
        )],
        [HostDefaultPlugin::new("example.agent", "default")],
    );
    fs::write(
        temp.path().join(".lenso/host-catalog.json"),
        serde_json::to_vec(&host).unwrap(),
    )
    .unwrap();
    let base = lenso_app_authoring::inspect_plugin_root(temp.path())
        .unwrap()
        .revision()
        .as_str()
        .to_owned();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["mcp", "--root"])
        .arg(temp.path())
        .arg("--allow-changes")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let call = |stdin: &mut std::process::ChildStdin,
                stdout: &mut BufReader<std::process::ChildStdout>,
                request: serde_json::Value|
     -> serde_json::Value {
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    let initialized = call(
        &mut stdin,
        &mut stdout,
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    assert_eq!(initialized["id"], 1);
    writeln!(
        stdin,
        "{}",
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    let preview_response = call(
        &mut stdin,
        &mut stdout,
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"project_change_preview","arguments":{"base_revision":base,"plugin_id":"example.agent","instance":"default","toml":"greeting = 'secret-value'\n"}}}),
    );
    assert!(preview_response["error"].is_null(), "{preview_response}");
    let preview_text = preview_response["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(!preview_text.contains("secret-value"));
    let preview: serde_json::Value = serde_json::from_str(preview_text).unwrap();
    assert_eq!(preview["status"], "ready");
    assert_eq!(preview["changed_fields"], serde_json::json!(["greeting"]));
    let path = temp.path().join("plugins/example.agent/default.toml");
    assert!(!path.exists());
    let digest = preview["proposal_digest"].as_str().unwrap();
    let apply = serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project_change_apply","arguments":{"proposal_digest":digest,"request_id":"apply-stdio"}}});
    let first = call(&mut stdin, &mut stdout, apply.clone());
    assert!(first["error"].is_null(), "{first}");
    let first_result: serde_json::Value =
        serde_json::from_str(first["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(first_result["state"], "published");
    assert_eq!(first_result["activation"], "not_observed");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "greeting = 'secret-value'\n"
    );
    let replay = call(&mut stdin, &mut stdout, apply);
    let replay_result: serde_json::Value =
        serde_json::from_str(replay["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(replay_result, first_result);
    assert_required_selection_rejected(
        &mut stdin,
        &mut stdout,
        &first_result["revision"],
        temp.path(),
    );
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

fn assert_required_selection_rejected(
    stdin: &mut std::process::ChildStdin,
    stdout: &mut BufReader<std::process::ChildStdout>,
    revision: &serde_json::Value,
    root: &std::path::Path,
) {
    writeln!(stdin, "{}", serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"project_selection_preview","arguments":{"base_revision":revision,"plugin_id":"example.agent","instance":"default","enabled":false}}})).unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let response: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(response["error"].is_null(), "{response}");
    let preview: serde_json::Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(preview["status"], "rejected");
    assert_eq!(
        preview["diagnostic_codes"],
        serde_json::json!(["required_instance_disabled"])
    );
    assert!(!root.join("plugins/example.agent/default.disabled").exists());
}

#[test]
fn stdio_selection_preview_and_apply_disable_then_enable() {
    use lenso_app_plan::authoring::{HostCatalog, HostPluginRelease, HostSlot, PluginDescriptor};

    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".lenso")).unwrap();
    let host = HostCatalog::new(
        [HostSlot::many("agent")],
        [HostPluginRelease::new(PluginDescriptor::new(
            "example.agent",
            "1.0.0",
            "agent",
        ))],
        [],
    );
    fs::write(
        temp.path().join(".lenso/host-catalog.json"),
        serde_json::to_vec(&host).unwrap(),
    )
    .unwrap();
    fs::create_dir_all(temp.path().join("plugins/example.agent")).unwrap();
    fs::write(temp.path().join("plugins/example.agent/default.toml"), "").unwrap();
    let base = lenso_app_authoring::inspect_plugin_root(temp.path())
        .unwrap()
        .revision()
        .as_str()
        .to_owned();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args(["mcp", "--root"])
        .arg(temp.path())
        .arg("--allow-changes")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let initialized = mcp_roundtrip(
        &mut stdin,
        &mut stdout,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    assert_eq!(initialized["id"], 1);
    writeln!(
        stdin,
        "{}",
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    let marker = temp.path().join("plugins/example.agent/default.disabled");
    let mut revision = base;
    for (enabled, request_id) in [(false, "disable"), (true, "enable")] {
        let preview = mcp_roundtrip(
            &mut stdin,
            &mut stdout,
            &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"project_selection_preview","arguments":{"base_revision":revision,"plugin_id":"example.agent","instance":"default","enabled":enabled}}}),
        );
        assert!(preview["error"].is_null(), "{preview}");
        let preview: serde_json::Value =
            serde_json::from_str(preview["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(preview["status"], "ready");
        assert_eq!(preview["requested_enabled"], enabled);
        let applied = mcp_roundtrip(
            &mut stdin,
            &mut stdout,
            &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project_change_apply","arguments":{"proposal_digest":preview["proposal_digest"],"request_id":request_id}}}),
        );
        assert!(applied["error"].is_null(), "{applied}");
        let applied: serde_json::Value =
            serde_json::from_str(applied["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(applied["state"], "published");
        assert_eq!(applied["kind"], "lenso.mcp-selection-apply");
        assert_eq!(marker.exists(), !enabled);
        revision = applied["revision"].as_str().unwrap().to_owned();
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn stdio_authorized_run_reaches_real_host_readiness_and_stops() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&root)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(created.status.success());
    let built = Command::new(cli)
        .args(["app", "build", "--root"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let mut child = Command::new(cli)
        .args(["mcp", "--root"])
        .arg(&root)
        .arg("--allow-run")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    mcp_roundtrip(
        &mut stdin,
        &mut stdout,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    writeln!(
        stdin,
        "{}",
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    let started = mcp_roundtrip(
        &mut stdin,
        &mut stdout,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"project_run","arguments":{"request_id":"real-empty-host","timeout_seconds":30}}}),
    );
    assert!(started["error"].is_null(), "{started}");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let status = run_status(&mut stdin, &mut stdout);
        if status["state"] == "running" {
            break;
        }
        assert!(status["state"] == "starting", "{status}");
        assert!(Instant::now() < deadline, "real Host did not become ready");
        std::thread::sleep(Duration::from_millis(25));
    }
    let observed = mcp_roundtrip(
        &mut stdin,
        &mut stdout,
        &serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"project_facts","arguments":{}}}),
    );
    assert!(observed["error"].is_null(), "{observed}");
    let observed: serde_json::Value =
        serde_json::from_str(observed["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(observed["status"], "resolved");
    assert_eq!(observed["runtime"]["status"], "running");
    assert_eq!(
        observed["root"],
        root.join("dist").canonicalize().unwrap().to_str().unwrap()
    );
    let stopped = mcp_roundtrip(
        &mut stdin,
        &mut stdout,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"project_run_stop","arguments":{"request_id":"real-empty-host"}}}),
    );
    assert!(stopped["error"].is_null(), "{stopped}");
    loop {
        let status = run_status(&mut stdin, &mut stdout);
        if status["state"] == "stopped" {
            break;
        }
        assert!(status["state"] == "stopping", "{status}");
        assert!(Instant::now() < deadline, "real Host did not stop");
        std::thread::sleep(Duration::from_millis(25));
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

fn run_status(
    stdin: &mut std::process::ChildStdin,
    stdout: &mut BufReader<std::process::ChildStdout>,
) -> serde_json::Value {
    let response = mcp_roundtrip(
        stdin,
        stdout,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project_run_status","arguments":{"request_id":"real-empty-host"}}}),
    );
    assert!(response["error"].is_null(), "{response}");
    serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

fn mcp_roundtrip(
    stdin: &mut std::process::ChildStdin,
    stdout: &mut BufReader<std::process::ChildStdout>,
    request: &serde_json::Value,
) -> serde_json::Value {
    writeln!(stdin, "{request}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}
