use std::path::Path;

use lenso_contract_codegen::{
    CodegenError, CompatibilityError, check_generated, generate, lint_compatibility,
    load_descriptor, round_trip_portable_json, validate_wire_value, write_generated,
};
use serde_json::json;

const FIXTURE: &str = "tests/fixtures/profile/capability.json";
const STREAM_FIXTURE: &str = "tests/fixtures/stream/capability.json";

#[test]
fn stream_descriptors_generate_bidirectional_rust_and_typescript_bindings() {
    let artifacts = generate(Path::new(STREAM_FIXTURE)).expect("stream Descriptor should generate");

    assert!(artifacts.rust.contains("impl StreamCapability"));
    assert!(artifacts.rust.contains("NativeStreamEndpoint"));
    assert!(artifacts.rust.contains("NativeStreamHandle"));
    assert!(artifacts.rust.contains("StreamEvent"));
    assert!(artifacts.rust.contains("fn chat("));
    assert!(artifacts.typescript.contains("StreamEvent"));
    assert!(artifacts.typescript.contains("send(message:"));
    assert!(artifacts.typescript.contains("receive(): Promise"));
    assert!(artifacts.typescript.contains(
        "chat(context: InvocationContext, request: ChatRequest): Promise<{ readonly ok: true; readonly value: StreamSession<ChatResponse, ChatError> } | { readonly ok: false; readonly error: ChatError }>;"
    ));
}

#[allow(clippy::too_many_lines)]
#[test]
fn one_descriptor_generates_matching_rust_and_typescript_bindings() {
    let artifacts = generate(Path::new(FIXTURE)).expect("profile Descriptor should generate");

    assert_eq!(artifacts.metadata.capability_id, "example.profile@1");
    assert_eq!(artifacts.metadata.descriptor_version, "1.0.0");
    assert!(artifacts.metadata.portable);
    assert!(
        artifacts
            .rust
            .contains("pub const CAPABILITY_ID: &str = \"example.profile@1\";")
    );
    assert!(artifacts.rust.contains("pub type Int64 = String;"));
    assert!(artifacts.rust.contains("pub type Uint64 = String;"));
    assert!(artifacts.rust.contains("pub type Bytes = String;"));
    assert!(
        artifacts
            .rust
            .contains("pub type OptionalValue<T> = Option<Option<T>>;")
    );
    assert!(artifacts.rust.contains("pub signed: Int64"));
    assert!(artifacts.rust.contains("pub enum RoundTripRequestKind"));
    assert!(artifacts.rust.contains("pub unsigned: Uint64"));
    assert!(artifacts.rust.contains("pub payload: Bytes"));
    assert!(artifacts.rust.contains("pub timestamp: Timestamp"));
    assert!(artifacts.rust.contains("pub duration: Duration"));
    assert!(artifacts.rust.contains("pub values: Vec<i64>"));
    assert!(
        artifacts
            .rust
            .contains("pub nullable_values: Option<Vec<Option<String>>>")
    );
    assert!(
        artifacts
            .rust
            .contains("pub nullable_map: Option<std::collections::BTreeMap<String, Option<i64>>>")
    );
    assert!(
        artifacts
            .rust
            .contains("pub optional_note: OptionalValue<String>")
    );
    assert!(artifacts.rust.contains("UnknownDomainError"));
    assert!(artifacts.rust.contains("serde::Serialize"));
    assert!(artifacts.rust.contains("encode_round_trip_request"));
    assert!(artifacts.rust.contains("decode_round_trip_error"));
    assert!(
        artifacts
            .rust
            .contains("RateLimited { payload: RoundTripErrorRateLimitedPayload }")
    );
    assert!(
        artifacts
            .typescript
            .contains("export const CAPABILITY_ID = \"example.profile@1\";")
    );
    assert!(
        artifacts
            .typescript
            .contains("export type Int64 = string &")
    );
    assert!(
        artifacts
            .typescript
            .contains("export type Bytes = string &")
    );
    assert!(artifacts.typescript.contains("signed: Int64"));
    assert!(artifacts.typescript.contains("kind?: \"alpha\" | \"beta\""));
    assert!(artifacts.typescript.contains("unsigned: Uint64"));
    assert!(artifacts.typescript.contains("payload: Bytes"));
    assert!(artifacts.typescript.contains("timestamp: Timestamp"));
    assert!(artifacts.typescript.contains("duration: Duration"));
    assert!(artifacts.typescript.contains("values: Array<number>"));
    assert!(
        artifacts
            .typescript
            .contains("nullable_values?: Array<string | null>")
    );
    assert!(
        artifacts
            .typescript
            .contains("nullable_map?: Record<string, number | null>")
    );
    assert!(
        artifacts
            .typescript
            .contains("optional_note?: string | null")
    );
    assert!(
        artifacts
            .typescript
            .contains("export type UnknownDomainError")
    );
    assert!(artifacts.typescript.contains("export type RuntimeFailure"));
    assert!(
        artifacts
            .typescript
            .contains("export interface InvocationContext")
    );
    assert!(artifacts.typescript.contains("encodeRoundTripRequest"));
    assert!(
        artifacts
            .typescript
            .contains("round_trip(context: InvocationContext")
    );
    assert!(artifacts.typescript.contains("Promise<RoundTripResult>"));
    assert!(
        artifacts
            .typescript
            .contains("readonly payload: RoundTripErrorRateLimitedPayload")
    );
    assert!(
        artifacts
            .typescript
            .contains("readonly ok: false; readonly error: RoundTripError")
    );
    assert!(
        artifacts
            .typescript
            .contains("readonly code: \"rate_limited\"")
    );
    assert!(
        artifacts
            .rust
            .contains("OptionalData { payload: OptionalValue<String> }")
    );
    assert!(
        artifacts
            .typescript
            .contains("readonly code: \"optional_data\"; readonly payload?: string | null")
    );
}

#[test]
fn generation_is_deterministic_and_metadata_does_not_depend_on_module_version() {
    let first = generate(Path::new(FIXTURE)).expect("first generation should succeed");
    let second = generate(Path::new(FIXTURE)).expect("second generation should succeed");

    assert_eq!(first.rust, second.rust);
    assert_eq!(first.typescript, second.typescript);
    assert!(!first.rust.contains("9.4.2"));
    assert!(!first.typescript.contains("9.4.2"));
    assert_eq!(first.metadata.capability_id, "example.profile@1");
    assert_eq!(first.metadata.descriptor_version, "1.0.0");
}

#[test]
fn additive_minor_evolution_is_accepted() {
    let result = lint_compatibility(
        Path::new("tests/fixtures/compatibility/base.json"),
        Path::new("tests/fixtures/compatibility/additive-minor.json"),
    );

    assert!(result.expect("documented additive minor changes should pass"));
}

#[test]
fn breaking_minor_evolution_requires_a_new_major_identity() {
    let error = lint_compatibility(
        Path::new("tests/fixtures/compatibility/base.json"),
        Path::new("tests/fixtures/compatibility/breaking-minor.json"),
    )
    .expect_err("breaking minor changes must be rejected");

    assert!(matches!(error, CompatibilityError::BreakingChanges { .. }));
}

#[test]
fn patch_evolution_cannot_change_the_observable_contract() {
    let error = lint_compatibility(
        Path::new("tests/fixtures/compatibility/base.json"),
        Path::new("tests/fixtures/compatibility/patch-with-change.json"),
    )
    .expect_err("patch changes must be rejected");

    assert!(matches!(error, CompatibilityError::BreakingChanges { .. }));
}

#[test]
fn descriptor_validation_keeps_capability_major_independent_from_semver() {
    let descriptor = load_descriptor(Path::new(FIXTURE)).expect("fixture should be valid");

    assert_eq!(descriptor.capability_id(), "example.profile@1");
    assert_eq!(descriptor.capability_major(), 1);
    assert_eq!(descriptor.version(), "1.0.0");
}

#[test]
fn shared_conformance_values_round_trip_without_precision_loss() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/portable-contract/conformance.json"
    ))
    .expect("the shared conformance corpus should be valid JSON");

    for value in corpus.as_array().expect("the corpus should be an array") {
        let round_tripped = round_trip_portable_json(&value["wire"])
            .expect("portable wire values should round-trip");
        assert_eq!(round_tripped, value["wire"]);
    }
}

#[test]
fn raw_wide_json_numbers_are_rejected_before_javascript_can_round_them() {
    let error = round_trip_portable_json(&json!(18_446_744_073_709_551_615_u64))
        .expect_err("uint64 must be encoded as a decimal string");

    assert!(error.to_string().contains("decimal-string"));
}

#[test]
fn wire_values_are_checked_against_portable_formats_and_schema_shape() {
    let schema = Path::new("tests/fixtures/profile/schemas/round-trip-request.schema.json");
    let valid = json!({
        "name": "Ada",
        "signed": "-9223372036854775808",
        "unsigned": "18446744073709551615",
        "payload": "AQI=",
        "timestamp": "2026-08-21T12:34:56.123Z",
        "duration": "PT1.5S",
        "values": [1, 2, 3],
        "nullable_values": ["one", null],
        "nullable_map": {"first": 1, "second": null},
        "optional_note": null
    });

    validate_wire_value(schema, &valid).expect("valid portable values should pass");

    let mut invalid = valid.clone();
    invalid["signed"] = json!("9223372036854775808");
    assert!(validate_wire_value(schema, &invalid).is_err());

    let mut invalid = valid.clone();
    invalid["payload"] = json!("not base64");
    assert!(validate_wire_value(schema, &invalid).is_err());

    let mut invalid = valid;
    invalid["optional_note"] = json!(42);
    assert!(validate_wire_value(schema, &invalid).is_err());

    let error_schema = Path::new("tests/fixtures/profile/schemas/round-trip-error.schema.json");
    let invalid_integer = json!({
        "code": "rate_limited",
        "payload": {"retry_after_ms": 9_007_199_254_740_992_i64}
    });
    assert!(validate_wire_value(error_schema, &invalid_integer).is_err());

    validate_wire_value(error_schema, &json!("future_variant"))
        .expect("unknown string Domain Errors should remain opaque");
    validate_wire_value(
        error_schema,
        &json!({"code": "future_variant", "payload": null}),
    )
    .expect("unknown object Domain Errors should remain opaque");
    assert!(
        validate_wire_value(
            error_schema,
            &json!({
                "code": "future_variant",
                "payload": {"unsafe_integer": 9_007_199_254_740_992_i64}
            })
        )
        .is_err()
    );
    assert!(
        validate_wire_value(
            error_schema,
            &json!({
                "code": "future_variant",
                "payload": 9_007_199_254_740_992.5_f64
            })
        )
        .is_err()
    );
}

#[test]
fn generated_artifact_check_detects_drift() {
    let root = std::env::temp_dir().join(format!("lenso-contract-codegen-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("the temporary artifact directory should exist");
    let rust_path = root.join("bindings.rs");
    let typescript_path = root.join("bindings.ts");

    write_generated(Path::new(FIXTURE), &rust_path, &typescript_path)
        .expect("the fixture should generate into the temporary directory");
    check_generated(Path::new(FIXTURE), &rust_path, &typescript_path)
        .expect("fresh generated artifacts should pass the drift check");

    std::fs::write(&rust_path, "stale\n")
        .expect("the test should be able to make the artifact stale");
    let error = check_generated(Path::new(FIXTURE), &rust_path, &typescript_path)
        .expect_err("drift must fail the check");
    assert!(matches!(error, CodegenError::GeneratedArtifactDrift { .. }));

    std::fs::remove_dir_all(root).expect("the temporary artifact directory should be removable");
}

#[test]
fn checked_in_profile_artifacts_are_current() {
    check_generated(
        Path::new(FIXTURE),
        Path::new("../../fixtures/portable-contract/generated/profile.rs"),
        Path::new("../../fixtures/portable-contract/generated/profile.ts"),
    )
    .expect("checked-in profile bindings should be generated from the fixture");
}

#[test]
fn local_recursive_refs_fail_with_a_diagnostic() {
    let root = std::env::temp_dir().join(format!(
        "lenso-contract-codegen-cycle-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("the temporary contract directory should exist");
    std::fs::write(
        root.join("cycle.schema.json"),
        r##"{"type":"object","properties":{"self":{"$ref":"#"}},"additionalProperties":false}"##,
    )
    .expect("the cyclic Schema should be writable");
    std::fs::write(
        root.join("error.schema.json"),
        r#"{"oneOf":[{"const":"failed"}]}"#,
    )
    .expect("the Domain Error Schema should be writable");
    let descriptor_path = root.join("capability.json");
    std::fs::write(
        &descriptor_path,
        r#"{"id":"example.cycle@1","version":"1.0.0","portable":true,"operations":[{"name":"read","interaction":"request","request_schema":"cycle.schema.json","response_schema":"cycle.schema.json","domain_error_schema":"error.schema.json"}]}"#,
    )
    .expect("the Descriptor should be writable");

    let error = load_descriptor(&descriptor_path).expect_err("recursive refs must be rejected");
    assert!(error.to_string().contains("cyclic"));
    std::fs::remove_dir_all(root).expect("the temporary contract directory should be removable");
}

#[test]
fn generated_value_schemas_reject_all_of_instead_of_dropping_constraints() {
    let root = std::env::temp_dir().join(format!(
        "lenso-contract-codegen-all-of-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("the temporary contract directory should exist");
    std::fs::write(
        root.join("request.schema.json"),
        r#"{"type":"object","allOf":[{"type":"object","properties":{"ignored":{"type":"string"}}}],"additionalProperties":false}"#,
    )
    .expect("the allOf Schema should be writable");
    std::fs::write(
        root.join("response.schema.json"),
        r#"{"type":"object","additionalProperties":false}"#,
    )
    .expect("the response Schema should be writable");
    std::fs::write(
        root.join("error.schema.json"),
        r#"{"oneOf":[{"const":"failed"}]}"#,
    )
    .expect("the Domain Error Schema should be writable");
    let descriptor_path = root.join("capability.json");
    std::fs::write(
        &descriptor_path,
        r#"{"id":"example.all-of@1","version":"1.0.0","portable":true,"operations":[{"name":"read","interaction":"request","request_schema":"request.schema.json","response_schema":"response.schema.json","domain_error_schema":"error.schema.json"}]}"#,
    )
    .expect("the Descriptor should be writable");

    let error = load_descriptor(&descriptor_path).expect_err("allOf must not be dropped");
    assert!(error.to_string().contains("allOf"));
    std::fs::remove_dir_all(root).expect("the temporary contract directory should be removable");
}

#[test]
fn operation_generated_names_cannot_shadow_prelude_types() {
    let root = std::env::temp_dir().join(format!(
        "lenso-contract-codegen-operation-name-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("the temporary contract directory should exist");
    for (name, schema) in [
        (
            "request.schema.json",
            r#"{"type":"object","additionalProperties":false}"#,
        ),
        (
            "response.schema.json",
            r#"{"type":"object","additionalProperties":false}"#,
        ),
        ("error.schema.json", r#"{"oneOf":[{"const":"failed"}]}"#),
    ] {
        std::fs::write(root.join(name), schema).expect("the Schema should be writable");
    }
    let descriptor_path = root.join("capability.json");
    std::fs::write(
        &descriptor_path,
        r#"{"id":"example.shadow@1","version":"1.0.0","portable":true,"operations":[{"name":"unknown_domain","interaction":"request","request_schema":"request.schema.json","response_schema":"response.schema.json","domain_error_schema":"error.schema.json"}]}"#,
    )
    .expect("the Descriptor should be writable");

    let error =
        load_descriptor(&descriptor_path).expect_err("generated prelude names must be reserved");
    assert!(error.to_string().contains("UnknownDomainError"));
    std::fs::remove_dir_all(root).expect("the temporary contract directory should be removable");
}

#[test]
fn operation_generated_names_cannot_shadow_client_methods() {
    let root = std::env::temp_dir().join(format!(
        "lenso-contract-codegen-client-method-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("the temporary contract directory should exist");
    for (name, schema) in [
        (
            "request.schema.json",
            r#"{"type":"object","additionalProperties":false}"#,
        ),
        (
            "response.schema.json",
            r#"{"type":"object","additionalProperties":false}"#,
        ),
        ("error.schema.json", r#"{"oneOf":[{"const":"failed"}]}"#),
    ] {
        std::fs::write(root.join(name), schema).expect("the Schema should be writable");
    }
    let descriptor_path = root.join("capability.json");
    std::fs::write(
        &descriptor_path,
        r#"{"id":"example.client-method@1","version":"1.0.0","portable":true,"operations":[{"name":"new","interaction":"request","request_schema":"request.schema.json","response_schema":"response.schema.json","domain_error_schema":"error.schema.json"}]}"#,
    )
    .expect("the Descriptor should be writable");

    let error = load_descriptor(&descriptor_path).expect_err("Client methods must be reserved");
    assert!(error.to_string().contains("generated Client API"));
    std::fs::remove_dir_all(root).expect("the temporary contract directory should be removable");
}

#[test]
fn stream_operation_generated_names_cannot_shadow_client_methods() {
    let root = std::env::temp_dir().join(format!(
        "lenso-contract-codegen-stream-client-method-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("the temporary contract directory should exist");
    for (name, schema) in [
        (
            "request.schema.json",
            r#"{"type":"object","additionalProperties":false}"#,
        ),
        (
            "response.schema.json",
            r#"{"type":"object","additionalProperties":false}"#,
        ),
        ("error.schema.json", r#"{"oneOf":[{"const":"failed"}]}"#),
    ] {
        std::fs::write(root.join(name), schema).expect("the Schema should be writable");
    }
    let descriptor_path = root.join("capability.json");
    std::fs::write(
        &descriptor_path,
        r#"{"id":"example.stream-client-method@1","version":"1.0.0","portable":true,"operations":[{"name":"new","interaction":"stream","request_schema":"request.schema.json","response_schema":"response.schema.json","domain_error_schema":"error.schema.json"}]}"#,
    )
    .expect("the Descriptor should be writable");

    let error = load_descriptor(&descriptor_path).expect_err("Client methods must be reserved");
    assert!(error.to_string().contains("generated Client API"));
    std::fs::remove_dir_all(root).expect("the temporary contract directory should be removable");
}

#[test]
fn operation_generated_names_cannot_shadow_types_from_other_operations() {
    let root = std::env::temp_dir().join(format!(
        "lenso-contract-codegen-operation-type-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("the temporary contract directory should exist");
    for (name, schema) in [
        (
            "request.schema.json",
            r#"{"type":"object","additionalProperties":false}"#,
        ),
        (
            "response.schema.json",
            r#"{"type":"object","additionalProperties":false}"#,
        ),
        ("error.schema.json", r#"{"oneOf":[{"const":"failed"}]}"#),
    ] {
        std::fs::write(root.join(name), schema).expect("the Schema should be writable");
    }
    let descriptor_path = root.join("capability.json");
    std::fs::write(
        &descriptor_path,
        r#"{"id":"example.operation-type@1","version":"1.0.0","portable":true,"operations":[{"name":"foo","interaction":"request","request_schema":"request.schema.json","response_schema":"response.schema.json","domain_error_schema":"error.schema.json"},{"name":"foo_invocation","interaction":"request","request_schema":"request.schema.json","response_schema":"response.schema.json","domain_error_schema":"error.schema.json"}]}"#,
    )
    .expect("the Descriptor should be writable");

    let error = load_descriptor(&descriptor_path).expect_err("generated type names must be unique");
    assert!(error.to_string().contains("FooInvocationError"));
    std::fs::remove_dir_all(root).expect("the temporary contract directory should be removable");
}
