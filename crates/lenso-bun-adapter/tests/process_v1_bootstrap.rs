#![cfg(unix)]

use std::{path::Path, process::Command, sync::Arc, thread, time::Duration};

use lenso_bun_adapter::process_v1::{
    ClientError, HostProcessLimits, ProcessV1ValidationError, ProcessV1ValueValidator,
    ShutdownDiagnostic, authenticate_process_v1, spawn_process_v1,
};
use lenso_process_protocol::{
    CapabilityDescriptor, HandshakeIdentity, InteractionKind, OperationDescriptor, PROCESS_PROFILE,
    PROVIDE_REQUEST_PROFILE, PeerLimits, ProcessOutcome, RequestParams, VALUE_PROFILE,
};
use nix::{sys::signal::kill, unistd::Pid};

#[test]
fn bun_reads_secret_and_writes_readiness_only_on_inherited_pipes() {
    let bun = std::env::var("BUN_BINARY").unwrap_or_else(|_| "bun".to_owned());
    if Command::new(&bun).arg("--version").output().is_err() {
        eprintln!("skipping Process V1 bootstrap smoke because Bun is unavailable");
        return;
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let mut command = Command::new(bun);
    command
        .arg(repository.join("fixtures/bun/process-v1-provider.ts"))
        .current_dir(repository);
    let mut spawned = spawn_process_v1(command, &HostProcessLimits::default()).unwrap();

    assert_eq!(spawned.readiness.protocol, PROCESS_PROFILE);
    assert_ne!(spawned.readiness.data_port, spawned.readiness.control_port);
    assert!(
        spawned
            .bootstrap_secret()
            .expose()
            .iter()
            .any(|byte| *byte != 0)
    );

    spawned.child_mut().kill().unwrap();
    spawned.child_mut().wait().unwrap();
}

#[test]
fn rust_host_authenticates_invokes_and_gracefully_shuts_down_bun() {
    let Some(command) = bun_command() else {
        eprintln!("skipping Process V1 cross-runtime smoke because Bun is unavailable");
        return;
    };
    let limits = HostProcessLimits::default();
    let spawned = spawn_process_v1(command, &limits).unwrap();
    let identity = identity();
    let session = authenticate_process_v1(spawned, identity.clone(), limits).unwrap();
    let result = session
        .request(
            RequestParams {
                session: session.session().to_owned(),
                correlation_id: "42".to_owned(),
                capability_id: "example.greeting@1".to_owned(),
                descriptor_version: "1.0.0".to_owned(),
                descriptor_digest: digest('d'),
                operation: "greet".to_owned(),
                interaction: InteractionKind::Request,
                caller_instance: None,
                remaining_timeout_nanos: None,
                extensions: Vec::new(),
                payload: serde_json::json!({"name": "Ada"}),
            },
            &GreetingValidator,
        )
        .unwrap();
    assert!(matches!(
        result.outcome,
        ProcessOutcome::Success { value }
            if value == serde_json::json!({"message": "Hello from Process V1, Ada!"})
    ));
    assert!(matches!(session.shutdown(), ShutdownDiagnostic::Clean(_)));
}

#[test]
fn rejected_host_identity_retires_and_reaps_the_child_group() {
    let Some(command) = bun_command() else {
        eprintln!("skipping Process V1 rejection smoke because Bun is unavailable");
        return;
    };
    let limits = HostProcessLimits::default();
    let spawned = spawn_process_v1(command, &limits).unwrap();
    let process = Pid::from_raw(i32::try_from(spawned.child().id()).unwrap());
    let mut invalid = identity();
    invalid.interaction_profiles.push("stream-v1".to_owned());
    assert!(matches!(
        authenticate_process_v1(spawned, invalid, limits),
        Err(ClientError::Protocol)
    ));
    assert!(kill(process, None).is_err());
}

#[test]
fn host_terminal_authority_wins_deadline_and_cancel_races() {
    let Some(command) = bun_command() else {
        eprintln!("skipping Process V1 terminal smoke because Bun is unavailable");
        return;
    };
    let limits = HostProcessLimits::default();
    let session = authenticate_process_v1(
        spawn_process_v1(command, &limits).unwrap(),
        identity(),
        limits,
    )
    .unwrap();
    assert_eq!(
        session.request(
            request_params(session.session(), "43", "Slow", Some("1000000")),
            &GreetingValidator
        ),
        Err(ClientError::DeadlineExceeded)
    );

    let session = Arc::new(session);
    let requester = Arc::clone(&session);
    let call = thread::spawn(move || {
        requester.request(
            request_params(requester.session(), "44", "Slow", None),
            &GreetingValidator,
        )
    });
    thread::sleep(Duration::from_millis(20));
    session.cancel("44".to_owned()).unwrap();
    assert_eq!(call.join().unwrap(), Err(ClientError::Cancelled));
    let session = Arc::try_unwrap(session).expect("request thread released the session");
    assert!(matches!(session.shutdown(), ShutdownDiagnostic::Clean(_)));
}

fn request_params(
    session: &str,
    correlation_id: &str,
    name: &str,
    remaining_timeout_nanos: Option<&str>,
) -> RequestParams {
    RequestParams {
        session: session.to_owned(),
        correlation_id: correlation_id.to_owned(),
        capability_id: "example.greeting@1".to_owned(),
        descriptor_version: "1.0.0".to_owned(),
        descriptor_digest: digest('d'),
        operation: "greet".to_owned(),
        interaction: InteractionKind::Request,
        caller_instance: None,
        remaining_timeout_nanos: remaining_timeout_nanos.map(str::to_owned),
        extensions: Vec::new(),
        payload: serde_json::json!({"name": name}),
    }
}

#[derive(Debug)]
struct GreetingValidator;

impl ProcessV1ValueValidator for GreetingValidator {
    fn capability_id(&self) -> &'static str {
        "example.greeting@1"
    }
    fn descriptor_version(&self) -> &'static str {
        "1.0.0"
    }
    fn descriptor_digest(&self) -> &'static str {
        "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    }
    fn validate_request(
        &self,
        operation: &str,
        value: &serde_json::Value,
    ) -> Result<(), ProcessV1ValidationError> {
        (operation == "greet" && value.get("name").is_some_and(serde_json::Value::is_string))
            .then_some(())
            .ok_or(ProcessV1ValidationError::Rejected)
    }
    fn validate_success(
        &self,
        operation: &str,
        value: &serde_json::Value,
    ) -> Result<(), ProcessV1ValidationError> {
        (operation == "greet"
            && value
                .get("message")
                .is_some_and(serde_json::Value::is_string))
        .then_some(())
        .ok_or(ProcessV1ValidationError::Rejected)
    }
    fn validate_domain_error(
        &self,
        operation: &str,
        _value: &serde_json::Value,
    ) -> Result<(), ProcessV1ValidationError> {
        (operation == "greet")
            .then_some(())
            .ok_or(ProcessV1ValidationError::Rejected)
    }
}

fn bun_command() -> Option<Command> {
    let bun = std::env::var("BUN_BINARY").unwrap_or_else(|_| "bun".to_owned());
    if Command::new(&bun).arg("--version").output().is_err() {
        return None;
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let mut command = Command::new(bun);
    command
        .arg(repository.join("fixtures/bun/process-v1-provider.ts"))
        .current_dir(repository);
    Some(command)
}

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn identity() -> HandshakeIdentity {
    HandshakeIdentity {
        protocol_profile: PROCESS_PROFILE.to_owned(),
        value_profile: VALUE_PROFILE.to_owned(),
        plugin_instance: "greeting".to_owned(),
        plugin_generation: "7".to_owned(),
        generation_spec_digest: digest('a'),
        artifact_digest: digest('b'),
        effective_host_grant_set_digest: digest('c'),
        interaction_profiles: vec![PROVIDE_REQUEST_PROFILE.to_owned()],
        provided_capabilities: vec![CapabilityDescriptor {
            capability_id: "example.greeting@1".to_owned(),
            descriptor_version: "1.0.0".to_owned(),
            descriptor_digest: digest('d'),
            operations: vec![OperationDescriptor {
                operation: "greet".to_owned(),
                interaction: InteractionKind::Request,
            }],
        }],
        outbound_bindings: Vec::new(),
        peer_limits: PeerLimits::v1_defaults(),
    }
}
