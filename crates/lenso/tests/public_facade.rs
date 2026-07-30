use lenso::{
    console::{ConsoleArea, ConsoleSurface},
    system_plane::{
        EnrollmentOffer, EnrollmentReceipt, VerifiedEnrollmentExchange, verify_enrollment_exchange,
    },
};

#[test]
fn public_facade_exposes_console_and_system_plane_authoring_boundaries() {
    assert_type::<ConsoleArea>();
    assert_type::<ConsoleSurface>();
    assert_type::<EnrollmentOffer>();
    assert_type::<EnrollmentReceipt>();
    assert_type::<VerifiedEnrollmentExchange>();
    let _ = verify_enrollment_exchange;
}

fn assert_type<T>() {}
