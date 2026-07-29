use lenso_service::system_plane::{
    CORE_PROTOCOL, CapabilityAdvertisement, CoreDocument, CoreIssueCode, core_document_schema,
    validate_core_document,
};
use serde_json::json;
use std::collections::BTreeSet;

fn digest(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn runtime_capability() -> CapabilityAdvertisement {
    CapabilityAdvertisement {
        contract_id: "lenso.system-plane.runtime-observability.v1".to_owned(),
        major_version: 1,
        feature_ids: BTreeSet::from(["function-runs".to_owned(), "queue-summary".to_owned()]),
        schema_digest: digest('a'),
        endpoint: "/system-plane/v1/runtime-observability".to_owned(),
    }
}

fn core_document() -> CoreDocument {
    CoreDocument {
        protocol: CORE_PROTOCOL.to_owned(),
        service_id: "support".to_owned(),
        service_principal: "service:support".to_owned(),
        service_revision: "release:sha256:0123456789abcdef".to_owned(),
        capabilities: vec![runtime_capability()],
    }
}

#[test]
fn core_document_is_a_strict_camel_case_wire_contract() {
    let value = serde_json::to_value(core_document()).unwrap();

    assert_eq!(value["protocol"], CORE_PROTOCOL);
    assert_eq!(value["serviceId"], "support");
    assert_eq!(value["servicePrincipal"], "service:support");
    assert_eq!(value["serviceRevision"], "release:sha256:0123456789abcdef");
    assert_eq!(
        value["capabilities"][0]["contractId"],
        "lenso.system-plane.runtime-observability.v1"
    );
    assert_eq!(
        value["capabilities"][0]["featureIds"],
        json!(["function-runs", "queue-summary"])
    );

    let mut with_unknown_field = value;
    with_unknown_field["consoleRoute"] = json!("/runtime");
    assert!(serde_json::from_value::<CoreDocument>(with_unknown_field).is_err());
}

#[test]
fn valid_core_document_has_no_issues() {
    assert!(validate_core_document(&core_document()).is_empty());

    let mut core_only = core_document();
    core_only.capabilities.clear();
    assert!(validate_core_document(&core_only).is_empty());
}

#[test]
fn validation_rejects_identity_protocol_and_revision_ambiguity() {
    let mut document = core_document();
    document.protocol = "lenso.system-plane.v2".to_owned();
    document.service_id.clear();
    document.service_principal.clear();
    document.service_revision.clear();

    let codes = validate_core_document(&document)
        .into_iter()
        .map(|issue| issue.code)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        codes,
        BTreeSet::from([
            CoreIssueCode::InvalidProtocol,
            CoreIssueCode::MissingServiceIdentity,
            CoreIssueCode::MissingServicePrincipal,
            CoreIssueCode::MissingServiceRevision,
        ])
    );
}

#[test]
fn validation_rejects_unverifiable_capability_advertisements() {
    let mut document = core_document();
    document.capabilities = vec![
        CapabilityAdvertisement {
            contract_id: "lenso.system-plane.v1".to_owned(),
            major_version: 0,
            feature_ids: BTreeSet::from(["Invalid Feature".to_owned()]),
            schema_digest: "not-a-digest".to_owned(),
            endpoint: "https://service.example/admin/runtime".to_owned(),
        },
        runtime_capability(),
        runtime_capability(),
    ];

    let codes = validate_core_document(&document)
        .into_iter()
        .map(|issue| issue.code)
        .collect::<BTreeSet<_>>();

    assert_eq!(
        codes,
        BTreeSet::from([
            CoreIssueCode::DuplicateCapability,
            CoreIssueCode::InvalidCapabilityContractId,
            CoreIssueCode::InvalidCapabilityMajorVersion,
            CoreIssueCode::InvalidEndpointReference,
            CoreIssueCode::InvalidFeatureId,
            CoreIssueCode::InvalidSchemaDigest,
        ])
    );
}

#[test]
fn capability_contract_id_major_must_match_the_advertised_major() {
    let mut document = core_document();
    document.capabilities[0].major_version = 2;

    let issues = validate_core_document(&document);

    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].code,
        CoreIssueCode::CapabilityMajorVersionMismatch
    );
    assert_eq!(issues[0].path, "$.capabilities[0].majorVersion");
}

#[test]
fn generated_schema_carries_core_wire_invariants() {
    let schema = core_document_schema();

    assert_eq!(schema["properties"]["protocol"]["const"], CORE_PROTOCOL);
    assert_eq!(schema["properties"]["serviceId"]["minLength"], 1);
    assert_eq!(
        schema["$defs"]["CapabilityAdvertisement"]["properties"]["majorVersion"]["minimum"],
        1
    );
    assert_eq!(
        schema["$defs"]["CapabilityAdvertisement"]["properties"]["schemaDigest"]["pattern"],
        "^sha256:[0-9a-f]{64}$"
    );

    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&serde_json::to_value(core_document()).unwrap()));

    let mut non_canonical = serde_json::to_value(core_document()).unwrap();
    non_canonical["capabilities"][0]["contractId"] =
        json!("lenso.system-plane.runtime--observability.v01");
    assert!(!validator.is_valid(&non_canonical));
}
