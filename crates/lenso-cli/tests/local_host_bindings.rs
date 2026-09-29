use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

fn success(command: &mut Command) -> Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

fn create_plugin(source: &Path) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let plugin = source.join("app/neutral.observer");
    fs::create_dir_all(plugin.join("src")).unwrap();
    let mut manifest = toml::toml! {
        [package]
        name = "neutral-host-observer"
        version = "0.1.0"
        edition = "2024"
        [package.metadata.lenso]
        plugin-id = "neutral.observer"
        root-slot = "observer"
        host-bindings = ["business-snapshot@1"]
        [package.metadata.lenso-cli]
        runtime = "native-linked"
        [dependencies]
        anyhow = "1"
        [workspace]
    };
    // Include the Host's Runner too: this checkout can precede its registry release.
    for name in [
        "lenso",
        "lenso-app-plan",
        "lenso-native-adapter",
        "lenso-runner",
    ] {
        manifest["dependencies"].as_table_mut().unwrap().insert(
            name.into(),
            toml::Value::Table(toml::Table::from_iter([(
                "path".into(),
                toml::Value::String(workspace.join("crates").join(name).display().to_string()),
            )])),
        );
    }
    fs::write(
        plugin.join("Cargo.toml"),
        toml::to_string(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        plugin.join("src/lib.rs"),
        include_str!("fixtures/local-host-binding/src/lib.rs"),
    )
    .unwrap();
}

fn assert_provenance(distribution: &Path) {
    let provenance = distribution.join(".lenso/generated-host");
    let source = fs::read_to_string(provenance.join("src/main.rs")).unwrap();
    assert!(source.contains("local_plugin_0::business_snapshot::bind("));
    assert!(source.contains("struct ManagedShutdownReceipt"));
    assert!(source.contains("receipt.publish()?"));
    assert!(!source.contains("mod local_business_snapshot"));
    assert!(!provenance.join("src/local_business_snapshot.rs").exists());
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(provenance.join("Cargo.toml")).unwrap()).unwrap();
    let dependencies = manifest["dependencies"].as_table().unwrap();
    assert!(!dependencies.contains_key("lenso-engine-authoring"));
    assert!(dependencies.values().all(|dependency| {
        dependency.get("package").and_then(toml::Value::as_str) != Some("lenso-engine-authoring")
    }));
    assert_eq!(
        dependencies["local_plugin_0"]["package"].as_str(),
        Some("neutral-host-observer")
    );
    assert!(
        manifest["patch"]["crates-io"]["lenso-native-adapter"]["path"]
            .as_str()
            .is_some_and(|path| Path::new(path).is_absolute())
    );
}

#[test]
fn selected_plugin_owns_binding_and_guard_lifetime_in_generated_host() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let distribution = temp.path().join("dist");
    let events = temp.path().join("events");
    let policy = temp.path().join("policy");
    let receipt = temp.path().join("shutdown-receipt");
    let token = "ab".repeat(32);
    let cli = env!("CARGO_BIN_EXE_lenso");
    success(
        Command::new(cli)
            .args(["app", "create"])
            .arg(&source)
            .args(["--runtime", "empty"]),
    );
    create_plugin(&source);
    success(
        Command::new(cli)
            .args(["app", "build", "--root"])
            .arg(&source)
            .arg("--out")
            .arg(&distribution),
    );
    assert_provenance(&distribution);

    for (mode, expected, diagnostic) in [
        ("absent", "activate\ndeactivate\n", None),
        (
            "valid",
            "bind\nactivate\nrecheck\nspawn\ndeactivate\ndrop\n",
            None,
        ),
        ("bind-failure", "bind\n", Some("fixture bind rejected")),
        (
            "recheck-failure",
            "bind\nactivate\nrecheck\ndeactivate\n",
            Some("fixture recheck rejected"),
        ),
    ] {
        fs::write(&events, "").unwrap();
        fs::write(&policy, mode).unwrap();
        let mut command = Command::new(cli);
        command
            .args(["app", "start", "--from"])
            .arg(&distribution)
            .arg("--check")
            .env("LENSO_MANAGED_SHUTDOWN_RECEIPT", &receipt)
            .env("LENSO_MANAGED_SHUTDOWN_TOKEN", &token)
            .env("LENSO_TEST_BINDING_EVENTS", &events);
        if mode != "absent" {
            command.arg("--business-snapshot-policy").arg(&policy);
        }
        let output = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!receipt.exists(), "--check must not acknowledge cleanup");
        assert_eq!(
            output.status.success(),
            diagnostic.is_none(),
            "{mode}: {stderr}"
        );
        assert_eq!(
            fs::read_to_string(&events).unwrap(),
            expected,
            "{mode}: {stderr}"
        );
        if let Some(diagnostic) = diagnostic {
            assert!(stderr.contains(diagnostic), "{mode}: {stderr}");
            assert!(!stderr.contains("Local App ready"), "{mode}: {stderr}");
            assert_failure_has_no_readiness(&distribution, &policy, &events, diagnostic, expected);
        } else {
            assert!(stderr.contains("Local App ready"), "{mode}: {stderr}");
            assert!(
                stderr.contains("Local App stopped cleanly"),
                "{mode}: {stderr}"
            );
        }
    }
    #[cfg(unix)]
    for mode in ["clean", "stop-failure", "panic", "forced"] {
        assert_shutdown_receipt(&distribution, &policy, &events, &token, mode);
    }
}

#[cfg(unix)]
fn assert_shutdown_receipt(
    distribution: &Path,
    policy: &Path,
    events: &Path,
    token: &str,
    mode: &str,
) {
    let ready = distribution.join(format!(".lenso/{mode}-ready"));
    let receipt = events.with_extension(format!("{mode}-receipt"));
    let log = events.with_extension(format!("{mode}-stderr"));
    fs::write(events, "").unwrap();
    fs::write(policy, "valid").unwrap();
    let mut child = Command::new(distribution.join(".lenso/host"))
        .arg("--business-snapshot-policy")
        .arg(policy)
        .arg("--ready-file")
        .arg(&ready)
        .env("LENSO_TEST_BINDING_EVENTS", events)
        .env("LENSO_TEST_BINDING_STOP", mode)
        .env("LENSO_MANAGED_SHUTDOWN_RECEIPT", &receipt)
        .env("LENSO_MANAGED_SHUTDOWN_TOKEN", token)
        .stdout(Stdio::null())
        .stderr(fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "{}",
            fs::read_to_string(&log).unwrap()
        );
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "Host did not become ready: {}",
                fs::read_to_string(&log).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(!receipt.exists(), "running generation must not acknowledge");
    let signal = if mode == "forced" { "-KILL" } else { "-TERM" };
    success(Command::new("kill").args([signal, &child.id().to_string()]));
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Host did not stop: {}", fs::read_to_string(&log).unwrap());
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(
        status.success(),
        mode == "clean",
        "{}",
        fs::read_to_string(&log).unwrap()
    );
    if mode != "clean" {
        assert!(!receipt.exists(), "{mode} must not acknowledge cleanup");
        return;
    }
    assert_eq!(fs::read(&receipt).unwrap(), token.as_bytes());
    assert_eq!(
        fs::read_to_string(events).unwrap(),
        "bind\nactivate\nrecheck\nspawn\ndeactivate\ndrop\n"
    );
}

fn assert_failure_has_no_readiness(
    distribution: &Path,
    policy: &Path,
    events: &Path,
    diagnostic: &str,
    expected: &str,
) {
    let marker = distribution.join(".lenso/test-ready");
    let log = events.with_extension("stderr");
    let receipt = events.with_extension("failure-receipt");
    fs::write(events, "").unwrap();
    let mut child = Command::new(distribution.join(".lenso/host"))
        .arg("--business-snapshot-policy")
        .arg(policy)
        .arg("--ready-file")
        .arg(&marker)
        .env("LENSO_TEST_BINDING_EVENTS", events)
        .env("LENSO_MANAGED_SHUTDOWN_RECEIPT", &receipt)
        .env("LENSO_MANAGED_SHUTDOWN_TOKEN", "cd".repeat(32))
        .stdout(Stdio::null())
        .stderr(fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "failed Host did not exit: {}",
                fs::read_to_string(log).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let stderr = fs::read_to_string(log).unwrap();
    assert!(!status.success(), "{stderr}");
    assert!(stderr.contains(diagnostic), "{stderr}");
    assert_eq!(fs::read_to_string(events).unwrap(), expected, "{stderr}");
    assert!(!marker.exists());
    assert!(!receipt.exists());
    assert!(!marker.with_extension("stage").exists());
    assert!(!stderr.contains("Local App ready"), "{stderr}");
}
