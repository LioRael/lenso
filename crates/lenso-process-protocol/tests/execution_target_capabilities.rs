use lenso_process_protocol::{
    EXECUTION_TARGET_CAPABILITY_PROFILE, ExecutionTargetCapability,
    ExecutionTargetCapabilityProfile,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Fixture {
    valid: Vec<ValidVector>,
    invalid: Vec<InvalidVector>,
}

#[derive(Deserialize)]
struct ValidVector {
    name: String,
    profile: Value,
    required: Vec<String>,
    missing: Vec<String>,
}

#[derive(Deserialize)]
struct InvalidVector {
    name: String,
    profile: Value,
    error: String,
}

#[test]
fn execution_target_capability_profiles_match_shared_conformance_vectors() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../fixtures/execution-target-capability-profile/conformance.json"
    ))
    .expect("the shared execution target capability fixture should be valid JSON");

    for vector in fixture.valid {
        let profile: ExecutionTargetCapabilityProfile = serde_json::from_value(vector.profile)
            .unwrap_or_else(|error| panic!("{}: {error}", vector.name));
        profile
            .validate()
            .unwrap_or_else(|error| panic!("{}: {error}", vector.name));
        let required = vector.required.iter().map(String::as_str);
        assert_eq!(
            profile.missing_capabilities(required),
            vector.missing,
            "{}",
            vector.name
        );
    }

    for vector in fixture.invalid {
        let result = serde_json::from_value::<ExecutionTargetCapabilityProfile>(vector.profile)
            .map_err(|error| error.to_string())
            .and_then(|profile| {
                profile
                    .validate()
                    .map(|()| profile)
                    .map_err(|error| error.to_string())
            });
        let error = result.expect_err(&vector.name);
        assert!(error.contains(&vector.error), "{}: {error}", vector.name);
    }
}

#[test]
fn invalid_profiles_and_unknown_requirements_fail_closed() {
    let invalid = ExecutionTargetCapabilityProfile {
        profile: "lenso.execution-target-capability-profile@2".to_owned(),
        target_profile: "example.invalid@1".to_owned(),
        capabilities: vec![ExecutionTargetCapability::Request],
    };
    assert!(!invalid.supports(ExecutionTargetCapability::Request));
    assert!(!invalid.supports_named("future-capability"));
    assert_eq!(
        invalid.missing_capabilities(["request", "future-capability"]),
        ["request", "future-capability"]
    );

    let valid = ExecutionTargetCapabilityProfile {
        profile: EXECUTION_TARGET_CAPABILITY_PROFILE.to_owned(),
        target_profile: "example.request-only@1".to_owned(),
        capabilities: vec![ExecutionTargetCapability::Request],
    };
    assert!(valid.supports(ExecutionTargetCapability::Request));
    assert!(!valid.supports_named("stream"));
    assert!(!valid.supports_named("future-capability"));
}
