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
    assert_eq!(responses.len(), 3, "{frames}");
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[1]["result"]["tools"][0]["name"], "project_facts");
    let facts: serde_json::Value = serde_json::from_str(
        responses[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(facts["kind"], "lenso.app-facts");
    assert_eq!(facts["schema_version"], 2);
    assert_eq!(facts["status"], "invalid");
    assert_eq!(facts["runtime"]["status"], "not_observed");
}
