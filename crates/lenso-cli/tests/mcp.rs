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
    assert_eq!(responses.len(), 4, "{frames}");
    let by_id = responses
        .iter()
        .map(|response| (response["id"].as_u64().unwrap(), response))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(by_id.len(), 4);
    let tools = by_id[&2]["result"]["tools"].as_array().unwrap();
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names, ["project_explain", "project_facts"].into());
    let facts: serde_json::Value =
        serde_json::from_str(by_id[&3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(facts["kind"], "lenso.app-facts");
    assert_eq!(facts["schema_version"], 2);
    assert_eq!(facts["status"], "invalid");
    assert_eq!(facts["runtime"]["status"], "not_observed");
    assert!(by_id[&4]["error"].is_object());
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
    assert_eq!(responses.len(), 2);
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
}
