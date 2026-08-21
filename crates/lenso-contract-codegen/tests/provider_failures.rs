use std::path::Path;

use lenso_contract_codegen::generate;

#[test]
fn request_providers_preserve_domain_and_runtime_failures() {
    let artifacts = generate(Path::new("tests/fixtures/profile/capability.json"))
        .expect("profile Descriptor should generate");

    assert!(
        artifacts
            .rust
            .contains("Result<RoundTripResponse, ProfileRoundTripInvocationError>")
    );
    assert!(
        artifacts.rust.contains(
            "Err(ProfileRoundTripInvocationError::Domain(error)) => Ok(Err(Box::new(error)"
        )
    );
    assert!(
        artifacts
            .rust
            .contains("Err(ProfileRoundTripInvocationError::Runtime(error)) => Err(error)")
    );
    assert!(artifacts.typescript.contains(
        "round_trip(context: InvocationContext, request: RoundTripRequest): Promise<RoundTripResult>;"
    ));
}

#[test]
fn stream_providers_preserve_domain_and_runtime_failures() {
    let artifacts = generate(Path::new("tests/fixtures/stream/capability.json"))
        .expect("stream Descriptor should generate");

    assert!(
        artifacts
            .rust
            .contains("Result<Box<dyn NativeStreamSession>, ConversationInvocationError>")
    );
    assert!(
        artifacts
            .rust
            .contains("Err(ConversationInvocationError::Runtime(error)) => Err(error)")
    );
    assert!(
        artifacts.typescript.contains(
            "chat(context: InvocationContext, request: ChatRequest): Promise<ChatResult>;"
        )
    );
}
