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
        r#"{"jsonrpc":"2.0","id":15,"method":"tools/call","params":{"name":"portable_catalog","arguments":{}}}"#,
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
    assert_eq!(responses.len(), 15, "{frames}");
    let by_id = responses
        .iter()
        .map(|response| (response["id"].as_u64().unwrap(), response))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(by_id.len(), 15);
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
    for id in [4, 5, 6, 9, 10, 11, 12, 13, 14, 15] {
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
            "portable_catalog",
            "project_check",
            "project_build",
            "project_build_cancel",
            "project_build_status",
            "project_change_apply",
            "project_change_preview",
            "project_explain",
            "project_facts",
            "project_linked_adopt",
            "project_linked_unadopt",
            "project_run",
            "project_run_status",
            "project_run_stop",
            "project_selection_preview",
        ]
        .into()
    );
}

#[test]
fn stdio_adopts_and_unadopts_only_the_fixed_signed_linked_crate() {
    use ed25519_dalek::SigningKey;
    use lenso_plugin_catalog::{
        Availability,
        linked_cargo::{LinkedCargoIntegration, LinkedCargoRelease, LinkedCargoSnapshot, sign},
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let cli = env!("CARGO_BIN_EXE_lenso");
    let created = Command::new(cli)
        .args(["app", "create"])
        .arg(&root)
        .args(["--runtime", "empty"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let archive = temp.path().join("example-web.crate");
    let archive_bytes = mcp_test_crate();
    fs::write(&archive, &archive_bytes).unwrap();
    let key = SigningKey::from_bytes(&[94; 32]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let release = LinkedCargoRelease {
        plugin_id: "example.web".into(),
        version: "0.4.5".into(),
        publisher_id: "test-publisher".into(),
        title: "Web".into(),
        summary: "Signed local fixture".into(),
        source_url: "https://example.com/source".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        package: "example-web-plugin".into(),
        registry_url: "https://crates.io".into(),
        crate_digest: lenso_plugin_catalog::digest(&archive_bytes),
        integration: LinkedCargoIntegration::LinkedPlugin,
        targets: vec![lenso_engine_authoring::native_host_target().into()],
        availability: Availability::Listed,
        documentation: Vec::new(),
    };
    let snapshot = temp.path().join("snapshot.json");
    fs::write(
        &snapshot,
        sign(
            &LinkedCargoSnapshot::new("test-catalog".into(), 1, now - 1, now + 3600, vec![release]),
            "test-key",
            &key,
        )
        .unwrap(),
    )
    .unwrap();
    let trust = temp.path().join("trust.json");
    fs::write(
        &trust,
        serde_json::to_vec(&serde_json::json!({
            "catalog_id": "test-catalog",
            "key_id": "test-key",
            "public_key_hex": hex::encode(key.verifying_key().as_bytes())
        }))
        .unwrap(),
    )
    .unwrap();

    #[cfg(unix)]
    {
        let alias = temp.path().join("archive-alias.crate");
        std::os::unix::fs::symlink(&archive, &alias).unwrap();
        let rejected = Command::new(cli)
            .args(["mcp", "--root"])
            .arg(&root)
            .arg("--linked-snapshot")
            .arg(&snapshot)
            .arg("--trust")
            .arg(&trust)
            .arg("--linked-crate")
            .arg(&alias)
            .arg("--allow-changes")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("regular file"));
    }

    let spawn_bridge = |allow_changes: bool| {
        let mut command = Command::new(cli);
        command.args(["mcp", "--root"]).arg(&root);
        command.arg("--linked-snapshot").arg(&snapshot);
        command.arg("--trust").arg(&trust);
        command.arg("--linked-crate").arg(&archive);
        if allow_changes {
            command.arg("--allow-changes");
        }
        let mut child = command
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
        (child, stdin, stdout)
    };
    let adopt = |request_id: &str| serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"project_linked_adopt","arguments":{"plugin_id":"example.web","version":"0.4.5","request_id":request_id}}});
    let (mut denied_child, mut denied_stdin, mut denied_stdout) = spawn_bridge(false);
    let denied = mcp_roundtrip(&mut denied_stdin, &mut denied_stdout, &adopt("denied"));
    assert!(denied["error"].is_object(), "{denied}");
    drop(denied_stdin);
    assert!(denied_child.wait().unwrap().success());
    assert!(!root.join("vendor/lenso/example.web/0.4.5").exists());

    fs::write(&archive, b"tampered archive").unwrap();
    let (mut bad_child, mut bad_stdin, mut bad_stdout) = spawn_bridge(true);
    let mismatched = mcp_roundtrip(&mut bad_stdin, &mut bad_stdout, &adopt("bad-digest"));
    let mismatched = mcp_tool_json(&mismatched);
    assert_eq!(mismatched["state"], "rejected");
    assert_eq!(
        mismatched["diagnostic_code"],
        "LENSO_ADOPTION_CRATE_DIGEST_MISMATCH"
    );
    assert!(!root.join("vendor/lenso/example.web/0.4.5").exists());
    drop(bad_stdin);
    assert!(bad_child.wait().unwrap().success());
    fs::write(&archive, &archive_bytes).unwrap();

    let (mut child, mut stdin, mut stdout) = spawn_bridge(true);
    let injected_path = mcp_roundtrip(
        &mut stdin,
        &mut stdout,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project_linked_adopt","arguments":{"plugin_id":"example.web","version":"0.4.5","request_id":"path","archive_path":"/tmp/other.crate"}}}),
    );
    assert_eq!(injected_path["result"]["isError"], true, "{injected_path}");
    let wrong_version = mcp_roundtrip(
        &mut stdin,
        &mut stdout,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"project_linked_adopt","arguments":{"plugin_id":"example.web","version":"0.4.6","request_id":"wrong-version"}}}),
    );
    let wrong_version = mcp_tool_json(&wrong_version);
    assert_eq!(wrong_version["state"], "rejected");
    assert_eq!(
        wrong_version["diagnostic_code"],
        "LENSO_ADOPTION_VERSION_NOT_LISTED"
    );
    assert!(!root.join("vendor/lenso/example.web/0.4.5").exists());

    fs::write(&archive, b"changed after MCP startup").unwrap();
    let selected = mcp_roundtrip(&mut stdin, &mut stdout, &adopt("adopt-1"));
    let selected = mcp_tool_json(&selected);
    assert_eq!(selected["state"], "selected");
    assert_eq!(selected["application"], "build_required");
    let source_lock = root.join("vendor/lenso/example.web/0.4.5/.lenso-linked-source.json");
    let original_lock = fs::read(&source_lock).unwrap();
    let original_config = fs::read(root.join("lenso.toml")).unwrap();
    assert_eq!(
        mcp_tool_json(&mcp_roundtrip(&mut stdin, &mut stdout, &adopt("adopt-1"))),
        selected
    );
    assert_eq!(
        mcp_tool_json(&mcp_roundtrip(&mut stdin, &mut stdout, &adopt("adopt-2")))["state"],
        "selected"
    );
    assert_eq!(fs::read(&source_lock).unwrap(), original_lock);
    assert_eq!(fs::read(root.join("lenso.toml")).unwrap(), original_config);
    let reused = mcp_roundtrip(
        &mut stdin,
        &mut stdout,
        &serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"project_linked_unadopt","arguments":{"plugin_id":"example.web","version":"0.4.5","request_id":"adopt-1"}}}),
    );
    assert!(reused["error"].is_object(), "{reused}");
    let removed = mcp_roundtrip(
        &mut stdin,
        &mut stdout,
        &serde_json::json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"project_linked_unadopt","arguments":{"plugin_id":"example.web","version":"0.4.5","request_id":"remove-1"}}}),
    );
    let removed = mcp_tool_json(&removed);
    assert_eq!(removed["state"], "unadopted");
    assert!(!root.join("vendor/lenso/example.web/0.4.5").exists());
    assert!(!root.join("plugins/example.web").exists());
    assert_eq!(
        mcp_tool_json(&mcp_roundtrip(
            &mut stdin,
            &mut stdout,
            &serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"project_linked_unadopt","arguments":{"plugin_id":"example.web","version":"0.4.5","request_id":"remove-1"}}}),
        )),
        removed
    );
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

fn mcp_test_crate() -> Vec<u8> {
    let manifest = "[package]\nname='example-web-plugin'\nversion='0.4.5'\nedition='2024'\n[package.metadata.lenso]\nplugin-id='example.web'\nroot-slot='tools'\n[dependencies]\nlenso='=0.5.25'\n";
    let source = b"#[lenso::plugin(consumer)]\n#[derive(Clone, Debug)]\nstruct Core {}\n";
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes) in [
        ("Cargo.toml", manifest.as_bytes()),
        ("src/lib.rs", source.as_slice()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(
                &mut header,
                format!("example-web-plugin-0.4.5/{name}"),
                bytes,
            )
            .unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn mcp_tool_json(response: &serde_json::Value) -> serde_json::Value {
    assert!(response["error"].is_null(), "{response}");
    serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn stdio_browses_signed_portable_metadata_without_installation_claim() {
    use ed25519_dalek::SigningKey;
    use lenso_plugin_catalog::{Artifact, Availability, Release, Snapshot, sign};
    use std::time::{SystemTime, UNIX_EPOCH};

    let temp = tempfile::tempdir().unwrap();
    let snapshot_path = temp.path().join("portable.snapshot.json");
    let trust_path = temp.path().join("portable.trust.json");
    let key = SigningKey::from_bytes(&[17; 32]);
    fs::write(
        &trust_path,
        serde_json::to_vec(&serde_json::json!({
            "catalog_id": "portable-test",
            "key_id": "test-key",
            "public_key_hex": hex::encode(key.verifying_key().as_bytes()),
        }))
        .unwrap(),
    )
    .unwrap();
    let make_release = |plugin_id: &str, availability| Release {
        plugin_id: plugin_id.into(),
        version: "1.0.0".into(),
        publisher_id: "test-publisher".into(),
        title: format!("{plugin_id} title"),
        summary: "Signed candidate metadata".into(),
        description: String::new(),
        presentation: None,
        source_url: "https://example.com/source".into(),
        source_revision: "a".repeat(40),
        license: "MIT".into(),
        artifact: Artifact {
            url: "https://example.com/plugin.lenso-plugin".into(),
            digest: format!("sha256:{}", "a".repeat(64)),
            size: 1,
            manifest_digest: format!("sha256:{}", "b".repeat(64)),
        },
        availability,
    };
    let mut releases = vec![
        make_release("example.available", Availability::Listed),
        make_release("example.withdrawn", Availability::Yanked),
        make_release("example.revoked", Availability::Revoked),
    ];
    releases[0].summary = "Signed candidate \u{202e}metadata".into();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    fs::write(
        &snapshot_path,
        sign(
            &Snapshot::new(
                "portable-test".into(),
                1,
                now - 120,
                now + 60,
                releases.clone(),
            ),
            "test-key",
            &key,
        )
        .unwrap(),
    )
    .unwrap();

    let cli = env!("CARGO_BIN_EXE_lenso");
    let command = Command::new(cli)
        .args(["plugins", "signed-search", "--snapshot"])
        .arg(&snapshot_path)
        .arg("--trust")
        .arg(&trust_path)
        .args(["--limit", "2", "--json"])
        .output()
        .unwrap();
    assert!(
        command.status.success(),
        "{}",
        String::from_utf8_lossy(&command.stderr)
    );
    let cli_page: serde_json::Value = serde_json::from_slice(&command.stdout).unwrap();
    assert!(
        cli_page["releases"][0]["summary"]
            .as_str()
            .unwrap()
            .contains('\u{202e}')
    );
    let human = Command::new(cli)
        .args(["plugins", "signed-search", "--snapshot"])
        .arg(&snapshot_path)
        .arg("--trust")
        .arg(&trust_path)
        .output()
        .unwrap();
    assert!(human.status.success());
    let human_text = String::from_utf8(human.stdout).unwrap();
    assert!(!human_text.contains('\u{202e}'));
    assert!(human_text.contains("\\u{202e}"));

    let mut child = Command::new(cli)
        .args(["mcp", "--root"])
        .arg(temp.path())
        .arg("--portable-snapshot")
        .arg(&snapshot_path)
        .arg("--portable-trust")
        .arg(&trust_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"portable_catalog","arguments":{"limit":2}}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"portable_catalog","arguments":{"query":"revoked"}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"portable_catalog","arguments":{"limit":21}}}"#,
    ]
    .join("\n");
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
    let by_id = responses
        .iter()
        .map(|response| (response["id"].as_u64().unwrap(), response))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(by_id.len(), 4, "{responses:?}");
    let page: serde_json::Value =
        serde_json::from_str(by_id[&2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(page, cli_page);
    assert_eq!(page["stale"], false);
    assert_eq!(page["history"], "not_checked");
    assert_eq!(page["target_compatibility"], "not_verified");
    assert_eq!(page["installation"], "not_authorized");
    assert_eq!(page["publisher_text_is_untrusted"], true);
    assert_eq!(page["total_releases"], 3);
    assert_eq!(page["next_offset"], 2);
    let revoked: serde_json::Value =
        serde_json::from_str(by_id[&3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(revoked["releases"][0]["availability"], "revoked");
    assert!(by_id[&4]["error"].is_object());

    fs::write(
        &snapshot_path,
        sign(
            &Snapshot::new(
                "portable-test".into(),
                2,
                now - 120,
                now - 1,
                releases.clone(),
            ),
            "test-key",
            &key,
        )
        .unwrap(),
    )
    .unwrap();
    let stale = Command::new(cli)
        .args(["plugins", "signed-search", "--snapshot"])
        .arg(&snapshot_path)
        .arg("--trust")
        .arg(&trust_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(stale.status.success());
    let stale_page: serde_json::Value = serde_json::from_slice(&stale.stdout).unwrap();
    assert_eq!(stale_page["stale"], true);
    fs::write(
        &snapshot_path,
        sign(
            &Snapshot::new("portable-test".into(), 3, now - 120, now + 60, releases),
            "test-key",
            &SigningKey::from_bytes(&[18; 32]),
        )
        .unwrap(),
    )
    .unwrap();
    let forged = Command::new(cli)
        .args(["plugins", "signed-search", "--snapshot"])
        .arg(&snapshot_path)
        .arg("--trust")
        .arg(&trust_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!forged.status.success());

    #[cfg(unix)]
    {
        use std::{os::unix::fs::symlink, path::Path};

        let check_rejected = |path: &Path| {
            let mut child = Command::new(cli)
                .args(["plugins", "signed-search", "--snapshot"])
                .arg(path)
                .arg("--trust")
                .arg(&trust_path)
                .arg("--json")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let started = Instant::now();
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(!status.success());
                    break;
                }
                if started.elapsed() > Duration::from_secs(3) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("signed catalog read blocked on a non-regular input");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        let symlink_path = temp.path().join("portable.symlink");
        symlink(&snapshot_path, &symlink_path).unwrap();
        check_rejected(&symlink_path);
        let fifo_path = temp.path().join("portable.fifo");
        let fifo = Command::new("mkfifo").arg(&fifo_path).output().unwrap();
        assert!(fifo.status.success());
        check_rejected(&fifo_path);
    }
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
    let before_build = call(
        &mut stdin,
        &mut stdout,
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"project_check","arguments":{"scope":"built_distribution"}}}),
    );
    assert!(before_build["error"].is_object(), "{before_build}");
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
    assert_eq!(
        final_status["output"],
        root.join("dist").canonicalize().unwrap().to_str().unwrap()
    );
    let checked_via_mcp = call(
        &mut stdin,
        &mut stdout,
        serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"project_check","arguments":{"scope":"built_distribution"}}}),
    );
    assert!(checked_via_mcp["error"].is_null(), "{checked_via_mcp}");
    let actual: serde_json::Value = serde_json::from_str(
        checked_via_mcp["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual, expected);

    let explained = Command::new(cli)
        .args(["app", "explain", "--json", "--root"])
        .arg(root.join("dist"))
        .output()
        .unwrap();
    assert!(explained.status.success());
    let expected_explanation: serde_json::Value =
        serde_json::from_slice(&explained.stdout).unwrap();
    let explained_via_mcp = call(
        &mut stdin,
        &mut stdout,
        serde_json::json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"project_explain","arguments":{"scope":"built_distribution"}}}),
    );
    assert!(explained_via_mcp["error"].is_null(), "{explained_via_mcp}");
    let actual_explanation: serde_json::Value = serde_json::from_str(
        explained_via_mcp["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual_explanation, expected_explanation);

    let facts = Command::new(cli)
        .args(["app", "facts", "--json", "--root"])
        .arg(root.join("dist"))
        .output()
        .unwrap();
    assert!(facts.status.success());
    let expected_facts: serde_json::Value = serde_json::from_slice(&facts.stdout).unwrap();
    let facts_via_mcp = call(
        &mut stdin,
        &mut stdout,
        serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"project_facts","arguments":{"scope":"built_distribution"}}}),
    );
    assert!(facts_via_mcp["error"].is_null(), "{facts_via_mcp}");
    let actual_facts: serde_json::Value = serde_json::from_str(
        facts_via_mcp["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual_facts, expected_facts);
    assert_eq!(actual_facts["root"], final_status["output"]);

    let source_configuration = root.join("plugins/local.starter/default.toml");
    let generated_configuration = root.join("dist/intent/plugins/local.starter/default.toml");
    let source_readme = root.join("README.md");
    let generated_authority = root.join("dist/intent/.lenso/host-build.json");
    let source_before = fs::read(&source_configuration).ok();
    let generated_before = fs::read(&generated_configuration).ok();
    let source_readme_before = fs::read(&source_readme).unwrap();
    let generated_authority_before = fs::read(&generated_authority).unwrap();
    let revision = &actual_facts["plugin_root_revision"];
    for (id, name, arguments) in [
        (
            10,
            "project_change_preview",
            serde_json::json!({"base_revision":revision,"plugin_id":"local.starter","instance":"default","toml":"greeting = 'source-only'\n"}),
        ),
        (
            11,
            "project_selection_preview",
            serde_json::json!({"base_revision":revision,"plugin_id":"local.starter","instance":"default","enabled":false}),
        ),
        (
            12,
            "project_change_apply",
            serde_json::json!({"proposal_digest":"sha256:unknown","request_id":"source-app-must-not-publish"}),
        ),
    ] {
        let rejected = call(
            &mut stdin,
            &mut stdout,
            serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
        );
        let message = rejected["error"]["message"].as_str().unwrap_or_default();
        assert!(message.contains("durable source plugins"), "{rejected}");
        assert!(message.contains("dist/intent"), "{rejected}");
    }
    assert_eq!(fs::read(&source_configuration).ok(), source_before);
    assert_eq!(fs::read(&generated_configuration).ok(), generated_before);
    assert_eq!(fs::read(&source_readme).unwrap(), source_readme_before);
    assert_eq!(
        fs::read(&generated_authority).unwrap(),
        generated_authority_before
    );

    let invalid_scope = call(
        &mut stdin,
        &mut stdout,
        serde_json::json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"project_check","arguments":{"scope":"../elsewhere"}}}),
    );
    assert_eq!(invalid_scope["result"]["isError"], true, "{invalid_scope}");
    let arbitrary_path = call(
        &mut stdin,
        &mut stdout,
        serde_json::json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"project_facts","arguments":{"scope":"built_distribution","path":"../elsewhere"}}}),
    );
    assert_eq!(
        arbitrary_path["result"]["isError"], true,
        "{arbitrary_path}"
    );
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[cfg(unix)]
#[test]
fn stdio_rejects_symlinked_fixed_root_distribution() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    fs::create_dir(&root).unwrap();
    symlink(temp.path(), root.join("dist")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lenso"))
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
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"project_facts","arguments":{"scope":"built_distribution"}}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"project_check","arguments":{"scope":"built_distribution"}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"project_explain","arguments":{"scope":"built_distribution"}}}"#,
    ]
    .join("\n");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{input}\n").as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let responses = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 4);
    for response in responses.iter().skip(1) {
        assert!(response["error"].is_object(), "{response}");
    }
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
