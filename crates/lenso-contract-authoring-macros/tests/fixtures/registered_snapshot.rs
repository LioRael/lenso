#![allow(dead_code)]

#[derive(lenso_contract_authoring::JsonSchema)]
struct Request {}

#[derive(lenso_contract_authoring_macros::DomainError)]
enum Error {
    Unavailable,
}

#[lenso_contract_authoring_macros::capability(
    id = "example.registered-snapshot",
    major = 1,
    version = "1.0.0",
    portable = true,
    cross_lane_transfer = false
)]
trait Registered {
    async fn read(
        &self,
        context: lenso_contract_authoring::Ctx<'_>,
        request: Request,
    ) -> Result<Request, Error>;
}

#[test]
fn old_codegen_accepts_the_default_snapshot_without_admission() {
    let snapshot: lenso_contract_authoring::CapabilitySnapshot =
        __lenso_capability_snapshot();
    let root = tempfile::tempdir().unwrap();
    let descriptor = root.path().join("capability.json");
    lenso_contract_codegen::write_source_snapshot(&snapshot, &descriptor).unwrap();
    lenso_contract_codegen::check_source_snapshot(&snapshot, &descriptor).unwrap();
    assert!(
        !std::fs::read_to_string(descriptor)
            .unwrap()
            .contains("request_admission")
    );
}
