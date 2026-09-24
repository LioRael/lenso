#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt as _,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use lenso_app_plan::authoring::{
    HostCatalog, HostDefaultPlugin, HostPluginRelease, HostSlot, PluginDescriptor,
};
use nix::{sys::signal::Signal, unistd::Pid};

#[test]
fn sigkill_of_supervisor_fences_a_second_cli_start() {
    let distribution = tempfile::tempdir().unwrap();
    let root = distribution.path();
    fs::create_dir(root.join(".lenso")).unwrap();
    fs::create_dir_all(root.join("intent/.lenso")).unwrap();
    fs::write(root.join(".lenso/host-mode"), b"native").unwrap();
    let host = root.join(".lenso/host");
    fs::write(
        &host,
        b"#!/bin/sh\nset -eu\nroot=$(CDPATH= cd -- \"$(dirname \"$0\")/..\" && pwd)\nwhile [ \"$1\" != --ready-file ]; do shift; done\nready=$2\nprintf '%s' \"$$\" > \"$root/host.pid\"\nprintf 'lenso.local-host-ready.v1\\n' > \"$ready\"\nexec sleep 8\n",
    )
    .unwrap();
    fs::set_permissions(&host, fs::Permissions::from_mode(0o700)).unwrap();
    let descriptor = PluginDescriptor::new("example.agent", "1.0.0", "agent")
        .with_configuration_schema(serde_json::json!({
            "type": "object",
            "properties": {"greeting": {"type": "string"}},
            "additionalProperties": false
        }));
    let catalog = HostCatalog::new(
        [HostSlot::one("agent")],
        [HostPluginRelease::new(descriptor)],
        [HostDefaultPlugin::new("example.agent", "default")],
    );
    fs::write(
        root.join(".lenso/host-catalog.json"),
        serde_json::to_vec(&catalog).unwrap(),
    )
    .unwrap();
    let snapshot = root.join("snapshot.json");
    fs::write(
        &snapshot,
        serde_json::to_vec(&serde_json::json!({
            "schema": "lenso.plugin-configuration-snapshot.v1",
            "revision": 1,
            "configurations": [{
                "plugin_id": "example.agent",
                "instance_key": "default",
                "toml": "greeting = 'hello'\n"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let policy = root.join("policy.json");
    fs::write(
        &policy,
        serde_json::to_vec(&serde_json::json!({
            "schema": "lenso.configuration-source-policy.v1",
            "source_reference": "crash-fence-test",
            "source": {"type": "file", "path": snapshot},
            "objects": [{
                "plugin_id": "example.agent",
                "instance_key": "default",
                "fields": ["greeting"]
            }],
            "max_stale_seconds": 20
        }))
        .unwrap(),
    )
    .unwrap();

    let cli = env!("CARGO_BIN_EXE_lenso");
    let ready = root.join("ready");
    let mut first = Command::new(cli)
        .args(["app", "start", "--from"])
        .arg(root)
        .arg("--configuration-policy")
        .arg(&policy)
        .arg("--ready-file")
        .arg(&ready)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait_for_file(&ready, &mut first);
    let host_pid: i32 = fs::read_to_string(root.join("host.pid"))
        .unwrap()
        .parse()
        .unwrap();
    nix::sys::signal::kill(
        Pid::from_raw(i32::try_from(first.id()).unwrap()),
        Signal::SIGKILL,
    )
    .unwrap();
    first.wait().unwrap();

    let mut second = Command::new(cli)
        .args(["app", "start", "--from"])
        .arg(root)
        .arg("--configuration-policy")
        .arg(&policy)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut timed_out = false;
    while second.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            timed_out = true;
            second.kill().unwrap();
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let output = second.wait_with_output().unwrap();
    stop_test_host(host_pid);
    assert!(!timed_out, "second CLI start was not fenced promptly");
    assert!(!output.status.success(), "second CLI start was accepted");
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("unconfirmed"), "{error}");
    assert!(
        root.join(".lenso/supervised-start.uncertain").is_file(),
        "crash fence must remain for operator recovery"
    );
    assert_eq!(
        fs::read_to_string(root.join("host.pid")).unwrap(),
        host_pid.to_string(),
        "a second Host was launched"
    );
}

fn wait_for_file(path: &std::path::Path, child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.is_file() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "first CLI start exited before readiness"
        );
        assert!(
            Instant::now() < deadline,
            "first CLI start did not become ready"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn stop_test_host(host_pid: i32) {
    let output = Command::new("ps")
        .args(["-o", "pgid=,command=", "-p", &host_pid.to_string()])
        .output()
        .unwrap();
    let process = String::from_utf8_lossy(&output.stdout);
    let mut fields = process.split_whitespace();
    let pid_text = host_pid.to_string();
    let group_matches = fields.next() == Some(pid_text.as_str());
    let command = fields.collect::<Vec<_>>().join(" ");
    if group_matches && (command == "sleep 8" || command.ends_with("/sleep 8")) {
        nix::sys::signal::kill(Pid::from_raw(host_pid), Signal::SIGKILL).unwrap();
    }
}
