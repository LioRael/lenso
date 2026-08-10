use lenso::{
    console::{ConsoleSurface, ConsoleSurfacePresentation, ConsoleUiArtifact},
    system_plane::{
        EnrollmentOffer, EnrollmentReceipt, VerifiedEnrollmentExchange, verify_enrollment_exchange,
    },
    workload_control::{
        WORKLOAD_CONTROL_PROTOCOL, WorkloadControlMessage, WorkloadObservationRequest,
        WorkloadReference,
    },
};

#[test]
fn public_facade_exposes_console_and_system_plane_authoring_boundaries() {
    assert_type::<ConsoleSurface>();
    assert_type::<ConsoleSurfacePresentation>();
    assert_type::<ConsoleUiArtifact>();
    assert_type::<EnrollmentOffer>();
    assert_type::<EnrollmentReceipt>();
    assert_type::<VerifiedEnrollmentExchange>();
    assert_type::<WorkloadReference>();
    assert_type::<WorkloadObservationRequest>();
    assert_type::<WorkloadControlMessage>();
    assert_eq!(WORKLOAD_CONTROL_PROTOCOL, "lenso.workload-control.v1");
    let _ = verify_enrollment_exchange;
}

fn assert_type<T>() {}
