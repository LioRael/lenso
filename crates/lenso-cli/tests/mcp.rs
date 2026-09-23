//! Exercise the public MCP transport, not only the underlying facts function.
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
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
    assert_eq!(responses.len(), 11, "{frames}");
    let by_id = responses
        .iter()
        .map(|response| (response["id"].as_u64().unwrap(), response))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(by_id.len(), 11);
    let tools = by_id[&2]["result"]["tools"].as_array().unwrap();
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
            "project_explain",
            "project_facts",
        ]
        .into()
    );
    let facts: serde_json::Value =
        serde_json::from_str(by_id[&3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(facts["kind"], "lenso.app-facts");
    assert_eq!(facts["schema_version"], 3);
    assert_eq!(facts["status"], "invalid");
    assert_eq!(facts["runtime"]["status"], "not_observed");
    assert!(by_id[&4]["error"].is_object());
    assert!(by_id[&5]["error"].is_object());
    assert!(by_id[&6]["error"].is_object());
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
    assert!(by_id[&9]["error"].is_object());
    assert!(by_id[&10]["error"].is_object());
    assert!(by_id[&11]["error"].is_object());
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
