#![cfg(unix)]

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    net::{SocketAddr, TcpListener, TcpStream},
    os::unix::fs::PermissionsExt as _,
    os::unix::process::CommandExt as _,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};

struct Dev(Child);

impl Dev {
    fn start(root: &Path, log: &Path) -> Self {
        Self(Self::command(root, log).spawn().unwrap())
    }

    fn command(root: &Path, log: &Path) -> Command {
        let output = File::create(log).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_lenso"));
        command
            .args(["plugin", "dev", "--watch", "--json", "--repo-root"])
            .arg(root)
            .env("CARGO_NET_OFFLINE", "true")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output));
        command
    }

    fn wait_for(&mut self, log: &Path, predicate: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            let output = fs::read_to_string(log).unwrap();
            assert!(
                self.0.try_wait().unwrap().is_none(),
                "watch exited:\n{output}"
            );
            if predicate(&output) {
                return output;
            }
            assert!(Instant::now() < deadline, "watch timed out:\n{output}");
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn stop(&mut self, log: &Path) {
        // Signal only the CLI, so cleanup cannot depend on terminal group delivery.
        kill(
            Pid::from_raw(self.0.id().try_into().unwrap()),
            Signal::SIGINT,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success(), "{}", fs::read_to_string(log).unwrap());
                return;
            }
            assert!(Instant::now() < deadline, "watch did not stop cleanly");
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for Dev {
    fn drop(&mut self) {
        // Let the CLI reap Cargo's separate process group before the fallback.
        let group = Pid::from_raw(self.0.id().try_into().unwrap());
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = kill(group, Signal::SIGINT);
            let deadline = Instant::now() + Duration::from_secs(6);
            while Instant::now() < deadline {
                if self.0.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
        let _ = killpg(group, Signal::SIGKILL);
        let _ = self.0.wait();
    }
}

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn addresses(output: &str) -> Vec<SocketAddr> {
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["kind"] == "lenso.web-dev-ready")
        .map(|event| {
            event["address"]
                .as_str()
                .unwrap()
                .strip_prefix("http://")
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect()
}

fn assert_http(address: SocketAddr, message: &str) {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let body = r#"{"name":"Ada"}"#;
    write!(
        stream,
        "POST /greetings HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 201"), "{response}");
    assert!(response.contains(message), "{response}");
    eprintln!(
        "HTTP at {address}: {} — {message}",
        response.lines().next().unwrap()
    );
}

#[test]
#[ignore = "downloads the pinned public Web SDK cohort and compiles real generated Hosts"]
fn web_watch_recovers_from_initial_and_later_compile_errors_without_leaking_listeners() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("company.watch");
    run(Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args([
            "plugin",
            "new",
            "company.watch",
            "--web",
            "--no-install",
            "--repo-root",
        ])
        .arg(temporary.path()));
    run(Command::new("cargo")
        .arg("generate-lockfile")
        .current_dir(&root));
    run(Command::new("cargo")
        .args(["fetch", "--locked"])
        .current_dir(&root));

    let route = root.join("src/routes/create.rs");
    let original = fs::read_to_string(&route).unwrap();
    let broken = original.replace(
        "let name = input.name.trim();",
        "let name = missing_watch_name;",
    );
    fs::write(&route, &broken).unwrap();
    let log = temporary.path().join("watch.log");
    let mut dev = Dev::start(&root, &log);
    let failure = dev.wait_for(&log, |log| log.contains("Waiting for source changes"));
    assert!(failure.contains("missing_watch_name"), "{failure}");
    assert!(
        failure.contains("generated Web handler source:"),
        "{failure}"
    );
    assert!(failure.contains("src/routes/create.rs:"), "{failure}");
    eprintln!(
        "Initial failure retained; {}",
        failure
            .lines()
            .find(|line| line.contains("generated Web handler source:"))
            .unwrap()
    );
    assert!(
        addresses(&failure).is_empty(),
        "a failed initial build must not listen"
    );

    fs::write(&route, &original).unwrap();
    let ready = dev.wait_for(&log, |log| addresses(log).len() == 1);
    let first = addresses(&ready)[0];
    assert_http(first, "Hello, Ada!");

    fs::write(&route, &broken).unwrap();
    dev.wait_for(&log, |log| {
        log.matches("Waiting for source changes").count() >= 2
    });
    // Reserving the old address proves the old listener was released and makes
    // a subsequent successful HTTP request evidence of a fresh Host.
    let retired = TcpListener::bind(first).expect("the old Host must release its listener");
    eprintln!("Failed rebuild released old listener {first}");
    fs::write(
        &route,
        original.replace("Hello, {name}!", "Recovered, {name}!"),
    )
    .unwrap();
    let ready = dev.wait_for(&log, |log| addresses(log).len() == 2);
    let second = addresses(&ready)[1];
    assert_ne!(first, second);
    assert_http(second, "Recovered, Ada!");
    dev.stop(&log);
    let stopped = TcpListener::bind(second).expect("Ctrl-C must release the current listener");
    eprintln!("CLI-only Ctrl-C released active listener {second}");
    drop((retired, stopped, dev));

    // Cancellation also remains responsive while waiting after a build error.
    fs::write(&route, broken).unwrap();
    let waiting_log = temporary.path().join("waiting.log");
    let mut waiting = Dev::start(&root, &waiting_log);
    waiting.wait_for(&waiting_log, |log| {
        log.contains("Waiting for source changes")
    });
    waiting.stop(&waiting_log);
    eprintln!("CLI-only Ctrl-C stopped failure-wait state cleanly");
}

#[test]
fn web_watch_cancels_initial_cargo_phases_with_their_children() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("company.cancel");
    run(Command::new(env!("CARGO_BIN_EXE_lenso"))
        .args([
            "plugin",
            "new",
            "company.cancel",
            "--web",
            "--no-install",
            "--repo-root",
        ])
        .arg(temporary.path()));
    for phase in ["metadata", "generate-lockfile", "build"] {
        let cargo = temporary.path().join(format!("cargo-{phase}"));
        fs::write(
            &cargo,
            r#"#!/bin/sh
if [ "$1" != "@PHASE@" ]; then
    if [ "$1" = metadata ]; then printf '{"target_directory":"%s/target"}\n' "$PWD"; fi
    exit 0
fi
trap 'kill "$sleeper" 2>/dev/null; wait "$sleeper"; exit 0' INT TERM
sleep 120 &
sleeper=$!
printf '%s\n%s\n%s\n' "$$" "$sleeper" "$PWD" > "$0.processes"
wait "$sleeper"
"#
            .replace("@PHASE@", phase),
        )
        .unwrap();
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
        let log = temporary.path().join(format!("{phase}.log"));
        let processes = temporary.path().join(format!("cargo-{phase}.processes"));
        let mut dev = Dev(Dev::command(&root, &log)
            .env("CARGO", &cargo)
            .spawn()
            .unwrap());
        dev.wait_for(&log, |_| {
            fs::read_to_string(&processes).is_ok_and(|text| text.lines().count() == 3)
        });
        let text = fs::read_to_string(processes).unwrap();
        let lines = text.lines().collect::<Vec<_>>();
        dev.stop(&log);
        for pid in &lines[..2] {
            assert_eq!(
                kill(Pid::from_raw(pid.parse().unwrap()), None),
                Err(nix::errno::Errno::ESRCH),
                "{phase} left process {pid} running"
            );
        }
        if phase != "metadata" {
            assert!(
                !Path::new(lines[2]).exists(),
                "cancelled Host staging must be removed"
            );
        }
        eprintln!("CLI-only Ctrl-C during {phase}: Cargo and child reaped; staging removed");
    }
}
