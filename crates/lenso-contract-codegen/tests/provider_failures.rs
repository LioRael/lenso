use std::path::Path;

use lenso_contract_codegen::generate;

#[test]
fn request_providers_preserve_domain_and_runtime_failures() {
    let artifacts = generate(Path::new("tests/fixtures/profile/capability.json"))
        .expect("profile Descriptor should generate");

    assert!(
        artifacts
            .rust
            .contains("NativeRequestFuture<ProfileRoundTrip>")
    );
    assert!(
        artifacts
            .rust
            .contains(".map_err(|error| Box::new(error) as Box<dyn std::any::Any>)")
    );
    assert!(
        artifacts
            .rust
            .contains("Rc::clone(&typed_endpoint.provider).round_trip(context, request)")
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
            "chat(context: InvocationContext, request: ChatRequest): ChatProviderOutput;"
        )
    );
}
