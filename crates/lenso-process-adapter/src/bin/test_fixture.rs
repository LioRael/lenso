use lenso_process_sdk::{ProcessOutcome, ProcessPlugin};
use serde_json::{Value, json};

#[derive(Debug)]
struct Echo;

impl ProcessPlugin for Echo {
    fn descriptor(&self) -> Value {
        json!({
            "abi": "lenso.json-request@1",
            "capabilities": [{
                "capability_id": "example.echo@1",
                "descriptor_version": "1.0.0",
                "request_operations": ["echo"],
            }],
        })
    }

    fn invoke(&self, capability: &str, operation: &str, request: Value) -> ProcessOutcome {
        if capability == "example.echo@1" && operation == "echo" {
            #[cfg(unix)]
            if request.get("malformed_result") == Some(&Value::Bool(true)) {
                // This mode must be the first invocation: intentionally bypass the SDK
                // with two terminal values, then still honor ordinary Host shutdown.
                println!(r#"{{"type":"result","id":1,"ok":true,"error":true}}"#);
                let mut line = String::new();
                // The SDK holds StdinLock while invoking; read the next unbuffered frame.
                let mut input = std::io::BufReader::new(std::fs::File::open("/dev/stdin").unwrap());
                std::io::BufRead::read_line(&mut input, &mut line).unwrap();
                assert_eq!(
                    serde_json::from_str::<Value>(&line).unwrap()["type"],
                    "shutdown"
                );
                std::process::exit(0);
            }
            if request.get("domain_error") == Some(&Value::Bool(true)) {
                return ProcessOutcome::DomainError(json!({"kind": "rejected"}));
            }
            if let Some(path) = request.get("crash_log").and_then(Value::as_str) {
                use std::io::Write as _;
                let mut log = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .unwrap();
                writeln!(log, "invoked").unwrap();
                std::process::exit(23);
            }
            if request.get("unicode_failure") == Some(&Value::Bool(true)) {
                return ProcessOutcome::Failure("界".repeat(200));
            }
            if let Some(milliseconds) = request.get("sleep_ms").and_then(Value::as_u64) {
                std::thread::sleep(std::time::Duration::from_millis(milliseconds));
            }
            ProcessOutcome::Success(request)
        } else {
            ProcessOutcome::DomainError(json!("not_found"))
        }
    }
}

fn main() {
    lenso_process_sdk::serve(&Echo).expect("serve Process Plugin fixture");
}
