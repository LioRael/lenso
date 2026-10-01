#![cfg(unix)]

#[path = "support/process_notes_source.rs"]
mod process_notes_source;
#[path = "support/process_notes_supervision.rs"]
mod process_notes_supervision;

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    net::{SocketAddr, TcpStream},
    os::unix::process::CommandExt as _,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};
use serde_json::Value;

struct ProcessGuard(Child);

impl ProcessGuard {
    fn spawn(command: &mut Command, log: &Path) -> Self {
        let output = File::create(log).unwrap();
        Self(
            command
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::from(output.try_clone().unwrap()))
                .stderr(Stdio::from(output))
                .spawn()
                .unwrap(),
        )
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let group = Pid::from_raw(self.0.id().try_into().unwrap());
        let _ = killpg(group, Signal::SIGINT);
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.0.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
        // Cargo and the Host can both have descendants; kill the entire group
        // even if its leader has already exited.
        let _ = killpg(group, Signal::SIGKILL);
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run(command: &mut Command, log: &Path) {
    let mut child = ProcessGuard::spawn(command, log);
    let deadline = Instant::now() + Duration::from_mins(15);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(
                status.success(),
                "{command:?}: {status}\n{}",
                fs::read_to_string(log).unwrap()
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{command:?} timed out\n{}",
            fs::read_to_string(log).unwrap()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn start(distribution: &Path, log: &Path) -> (ProcessGuard, SocketAddr) {
    let mut child = ProcessGuard::spawn(
        Command::new(env!("CARGO_BIN_EXE_lenso"))
            .args(["app", "start", "--from"])
            .arg(distribution),
        log,
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let output = fs::read_to_string(log).unwrap();
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "Host exited before readiness:\n{output}"
        );
        if let Some(address) = output
            .lines()
            .find_map(|line| line.strip_prefix("Listening on http://"))
        {
            return (child, address.trim_end_matches('/').parse().unwrap());
        }
        assert!(
            Instant::now() < deadline,
            "Host did not publish a listener:\n{output}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn assert_process_shutdown_receipt(distribution: &Path, log: &Path) {
    let private = tempfile::tempdir().unwrap();
    let ready = private.path().join("ready");
    let receipt = private.path().join("shutdown");
    let token = "ef".repeat(32);
    let mut child = ProcessGuard::spawn(
        Command::new(distribution.join(".lenso/host"))
            .args(["app", "__run-local", "--", "--ready-file"])
            .arg(&ready)
            .env("LENSO_MANAGED_SHUTDOWN_RECEIPT", &receipt)
            .env("LENSO_MANAGED_SHUTDOWN_TOKEN", &token),
        log,
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ready.exists() {
        assert!(
            child.0.try_wait().unwrap().is_none() && Instant::now() < deadline,
            "Host did not become ready:\n{}",
            fs::read_to_string(log).unwrap()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(!receipt.exists());
    // Signal only the Host; its Adapter must stop and reap the Process Plugin.
    kill(
        Pid::from_raw(child.0.id().try_into().unwrap()),
        Signal::SIGTERM,
    )
    .unwrap();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "{}", fs::read_to_string(log).unwrap());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Host did not stop:\n{}",
            fs::read_to_string(log).unwrap()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(
        fs::read(&receipt).unwrap_or_else(|error| panic!(
            "shutdown receipt missing: {error}\n{}",
            fs::read_to_string(log).unwrap()
        )),
        token.as_bytes()
    );
}

fn request(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String, Value) {
    request_with_content_type(address, method, path, body, Some("application/json"))
}

fn request_with_content_type(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: &str,
    content_type: Option<&str>,
) -> (u16, String, Value) {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let content_type = content_type
        .map(|value| format!("Content-Type: {value}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\n{content_type}\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (
        status,
        headers.to_ascii_lowercase(),
        serde_json::from_str(body).unwrap(),
    )
}

fn assert_problem(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: &str,
    code: &str,
    status: u16,
) {
    let (actual_status, headers, problem) = request(address, method, path, body);
    assert_eq!(actual_status, status, "{problem}");
    assert!(
        headers.contains("content-type: application/problem+json"),
        "{headers}"
    );
    assert_eq!(problem["status"], status);
    assert_eq!(problem["code"], code);
}

fn create_and_read(address: SocketAddr, expected_title: &str) {
    let (status, headers, created) = request(
        address,
        "POST",
        "/notes",
        r#"{"title":"First","body":"Hello"}"#,
    );
    assert_eq!(status, 201, "{created}");
    assert!(
        headers.contains("content-type: application/json"),
        "{headers}"
    );
    assert_eq!(created["title"], expected_title);
    assert_eq!(created["body"], "Hello");
    let id = created["id"].as_str().unwrap();
    assert!(!id.is_empty());
    let (status, _, found) = request(address, "GET", &format!("/notes/{id}"), "");
    assert_eq!(status, 200, "{found}");
    assert_eq!(found, created);
}

fn assert_input_problems(address: SocketAddr) {
    for content_type in [None, Some("text/plain")] {
        let (status, headers, problem) = request_with_content_type(
            address,
            "POST",
            "/notes",
            r#"{"title":"First","body":"Hello"}"#,
            content_type,
        );
        assert_eq!(status, 415, "{problem}");
        assert!(headers.contains("content-type: application/problem+json"));
        assert_eq!(problem["code"], "json_content_type_required");
    }
    for body in [
        "{",
        r#"{"title":3,"body":"Hello"}"#,
        r#"{"body":"Hello"}"#,
        r#"{"title":"First","body":"Hello","unexpected":true}"#,
    ] {
        assert_problem(address, "POST", "/notes", body, "invalid_json_body", 400);
    }
    assert_problem(
        address,
        "POST",
        "/notes",
        r#"{"title":"  ","body":"Hello"}"#,
        "invalid_title",
        400,
    );
    assert_problem(address, "GET", "/notes/missing", "", "note_not_found", 404);
}

fn build(source: &Path, distribution: &Path, log: &Path) {
    run(
        Command::new(env!("CARGO_BIN_EXE_lenso"))
            .args(["app", "build", "--root"])
            .arg(source)
            .arg("--out")
            .arg(distribution),
        log,
    );
    assert_eq!(
        fs::read_to_string(distribution.join(".lenso/host-mode")).unwrap(),
        "portable"
    );
}

fn assert_same_binary(left: &Path, right: &Path) {
    let mut left_file = File::open(left).unwrap();
    let mut right_file = File::open(right).unwrap();
    assert_eq!(
        left_file.metadata().unwrap().len(),
        right_file.metadata().unwrap().len(),
        "Host binary size changed: {} versus {}",
        left.display(),
        right.display()
    );
    let mut left_bytes = [0; 8192];
    let mut right_bytes = [0; 8192];
    loop {
        let count = left_file.read(&mut left_bytes).unwrap();
        if count == 0 {
            break;
        }
        right_file.read_exact(&mut right_bytes[..count]).unwrap();
        assert!(
            left_bytes[..count] == right_bytes[..count],
            "Host binary changed: {} versus {}",
            left.display(),
            right.display()
        );
    }
}

#[test]
fn ordinary_creation_keeps_the_registry_resolvable_starter() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("registry-notes");
    run(
        Command::new(env!("CARGO_BIN_EXE_lenso"))
            .arg("new")
            .arg(&source),
        &temp.path().join("create.log"),
    );
    assert!(source.join("Cargo.lock").is_file());
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(source.join("Cargo.toml")).unwrap()).unwrap();
    assert_eq!(
        manifest["dependencies"]["lenso-capability-http-endpoint"].as_str(),
        Some("=0.3.2")
    );
    assert_eq!(
        manifest["dependencies"]["lenso-process-sdk"].as_str(),
        Some("=0.2.0")
    );
    assert!(manifest.get("patch").is_none());
}

#[test]
fn typed_process_notes_rebuild_guest_and_reuse_precompiled_host() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("notes");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    let log = temp.path().join("command.log");
    let sdk = process_notes_source::endpoint_sdk_source();
    run(
        Command::new(env!("CARGO_BIN_EXE_lenso"))
            .arg("new")
            .arg(&source)
            .arg("--http-sdk-source")
            .arg(&sdk),
        &log,
    );
    assert!(source.join("Cargo.lock").is_file());
    let business_path = source.join("src/lib.rs");
    let business = fs::read_to_string(&business_path).unwrap();
    assert!(business.contains("#[endpoint(standalone)]"));
    let main = fs::read_to_string(source.join("src/main.rs")).unwrap();
    assert!(main.contains("::process::serve("));
    for forbidden in [
        "ProcessPlugin",
        "ProcessFrame",
        "descriptor_json",
        "schema_version",
    ] {
        assert!(
            !business.contains(forbidden) && !main.contains(forbidden),
            "business source contains {forbidden}"
        );
    }
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(source.join("Cargo.toml")).unwrap()).unwrap();
    assert_eq!(
        manifest["package"]["metadata"]["lenso-cli"]["runtime"].as_str(),
        Some("process")
    );
    assert!(manifest["dependencies"].get("lenso-process-sdk").is_none());
    let endpoint = &manifest["dependencies"]["lenso-capability-http-endpoint"];
    assert_eq!(endpoint["version"].as_str(), Some("=0.3.8"));
    assert_eq!(endpoint["path"].as_str(), sdk.to_str());
    assert!(
        endpoint["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|feature| feature.as_str() == Some("process"))
    );
    run(
        Command::new("cargo")
            .args(["test", "--manifest-path"])
            .arg(source.join("Cargo.toml"))
            .args(["--test", "notes"]),
        &log,
    );
    build(&source, &first, &log);
    let precompiled_host = Path::new(env!("CARGO_BIN_EXE_lenso"));
    assert_same_binary(precompiled_host, &first.join(".lenso/host"));
    let (running, address) = start(&first, &temp.path().join("first.log"));
    create_and_read(address, "First");
    assert_input_problems(address);
    drop(running);
    assert_process_shutdown_receipt(&first, &temp.path().join("shutdown.log"));
    process_notes_supervision::assert_update_and_restart(&first);

    let original = "title: input.title,";
    assert_eq!(business.matches(original).count(), 1);
    fs::write(
        business_path,
        business.replace(original, r#"title: format!("edited: {}", input.title),"#),
    )
    .unwrap();
    build(&source, &second, &log);
    assert_same_binary(precompiled_host, &second.join(".lenso/host"));
    assert_same_binary(precompiled_host, &first.join(".lenso/host"));
    fs::remove_dir_all(&source).unwrap();
    let (_running, address) = start(&second, &temp.path().join("second.log"));
    create_and_read(address, "edited: First");
    assert_problem(address, "GET", "/notes/missing", "", "note_not_found", 404);
}
