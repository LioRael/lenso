//! Exercise the build environment boundary in fresh processes, without changing
//! the test runner's process-wide environment or contacting a proxy.
use std::process::Command;

const NETWORK_KEYS: &[&str] = &[
    "CARGO_HTTP_PROXY",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];
const PROXY: &str = "http://127.0.0.1:9";
const CHECKED: &str = "build environment assertions completed";

#[test]
fn build_preserves_network_settings_without_business_credentials() {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "build_environment_probe", "--nocapture"])
        .env("LENSO_BUILD_ENV_PROBE", "1")
        .env("CARGO_BUILD_JOBS", "2")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .env("AWS_SECRET_ACCESS_KEY", "synthetic-business-secret")
        .env("DATABASE_URL", "synthetic-business-database");
    for name in NETWORK_KEYS {
        command.env(name, PROXY);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "environment boundary probe failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(CHECKED));
}

#[test]
fn build_environment_probe() {
    if std::env::var_os("LENSO_BUILD_ENV_PROBE").is_none() {
        return;
    }
    let output = lenso_engine_app::app::build_command(std::env::current_exe().unwrap())
        .args(["--exact", "environment_after_boundary", "--nocapture"])
        .env("LENSO_BUILD_ENV_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "filtered child failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(CHECKED));
    println!("{CHECKED}");
}

#[test]
fn environment_after_boundary() {
    if std::env::var_os("LENSO_BUILD_ENV_CHILD").is_none() {
        return;
    }
    for name in NETWORK_KEYS {
        assert_eq!(std::env::var(name).as_deref(), Ok(PROXY), "{name}");
    }
    for name in ["NO_PROXY", "no_proxy"] {
        assert_eq!(
            std::env::var(name).as_deref(),
            Ok("127.0.0.1,localhost"),
            "{name}"
        );
    }
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("2"));
    for name in [
        "AWS_SECRET_ACCESS_KEY",
        "DATABASE_URL",
        "LENSO_BUILD_ENV_PROBE",
    ] {
        assert!(std::env::var_os(name).is_none(), "must remove {name}");
    }
    println!("{CHECKED}");
}
