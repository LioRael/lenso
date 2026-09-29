#![allow(dead_code)]

use lenso_native_adapter::{plugin, provides};

mod management {
    #[macro_export]
    macro_rules! __test_management_provided_same_role {
        () => { r#"{"capability_id":"example.management@1","descriptor_version":"1.0.0","operations":["identity"],"operation_kinds":{},"default_admission":{"queue_capacity":0,"max_concurrency":1},"operation_admissions":{},"event_admission":null,"cross_lane_transfer":false}"# };
    }
    pub use crate::__test_management_provided_same_role as __lenso_provided_same_role;
    pub use crate::__test_management_lower_same_role as __lenso_native_lower_same_role;
    pub use crate::__test_management_lower_object_same_role as __lenso_native_lower_object_same_role;
    pub use crate::__test_same_role_endpoints as __lenso_native_endpoints_same_role;

    pub struct SameRole;
    pub trait SameRoleProvider {
        fn identity(&self) -> Result<&'static str, lenso_native_adapter::RuntimeFailure>;
    }
    #[macro_export]
    macro_rules! __test_management_lower_same_role {
        ($plugin:ty, $support:path) => {
            use $support as __LensoNativeSupportSameRole;
            impl $crate::management::SameRoleProvider for $plugin {
                fn identity(&self) -> Result<&'static str, __LensoNativeSupportSameRole::RuntimeFailure> {
                    Ok(self.management_identity())
                }
            }
        };
    }
    #[macro_export]
    macro_rules! __test_management_lower_object_same_role {
        ($object:ty, $plugin:ty, $support:path) => {
            use $support as __LensoNativeSupportSameRole;
            impl $crate::management::SameRoleProvider for $object {
                fn identity(&self) -> Result<&'static str, __LensoNativeSupportSameRole::RuntimeFailure> {
                    Ok(self.get()?.management_identity())
                }
            }
        };
    }
}

mod authentication {
    #[macro_export]
    macro_rules! __test_authentication_provided_same_role {
        () => { r#"{"capability_id":"example.authentication@1","descriptor_version":"1.0.0","operations":["identity"],"operation_kinds":{},"default_admission":{"queue_capacity":0,"max_concurrency":1},"operation_admissions":{},"event_admission":null,"cross_lane_transfer":false}"# };
    }
    pub use crate::__test_authentication_provided_same_role as __lenso_provided_same_role;
    pub use crate::__test_authentication_lower_same_role as __lenso_native_lower_same_role;
    pub use crate::__test_authentication_lower_object_same_role as __lenso_native_lower_object_same_role;
    pub use crate::__test_same_role_endpoints as __lenso_native_endpoints_same_role;

    pub struct SameRole;
    pub trait SameRoleProvider {
        fn identity(&self) -> Result<&'static str, lenso_native_adapter::RuntimeFailure>;
    }
    #[macro_export]
    macro_rules! __test_authentication_lower_same_role {
        ($plugin:ty, $support:path) => {
            use $support as __LensoNativeSupportSameRole;
            impl $crate::authentication::SameRoleProvider for $plugin {
                fn identity(&self) -> Result<&'static str, __LensoNativeSupportSameRole::RuntimeFailure> {
                    Ok(self.authentication_identity())
                }
            }
        };
    }
    #[macro_export]
    macro_rules! __test_authentication_lower_object_same_role {
        ($object:ty, $plugin:ty, $support:path) => {
            use $support as __LensoNativeSupportSameRole;
            impl $crate::authentication::SameRoleProvider for $object {
                fn identity(&self) -> Result<&'static str, __LensoNativeSupportSameRole::RuntimeFailure> {
                    Ok(self.get()?.authentication_identity())
                }
            }
        };
    }
}

#[macro_export]
macro_rules! __test_same_role_endpoints {
    ($provider:expr, $support:path) => {{
        use $support as __LensoNativeSupport;
        let _ = $provider;
        (
            Vec::<std::rc::Rc<dyn __LensoNativeSupport::NativeRequestEndpoint>>::new(),
            Vec::<std::rc::Rc<dyn __LensoNativeSupport::NativeStreamEndpoint>>::new(),
            Vec::<std::rc::Rc<dyn __LensoNativeSupport::NativeEventEndpoint>>::new(),
        )
    }};
}

#[plugin]
#[derive(Debug)]
struct Provider {}

#[provides(management::SameRole, authentication::SameRole)]
impl Provider {
    fn management_identity(&self) -> &'static str { "management" }
    fn authentication_identity(&self) -> &'static str { "authentication" }
}

#[test]
fn two_namespaces_with_the_same_role_keep_independent_projections() {
    let provider = Provider {};
    assert_eq!(management::SameRoleProvider::identity(&provider).unwrap(), "management");
    assert_eq!(authentication::SameRoleProvider::identity(&provider).unwrap(), "authentication");
    let descriptor: serde_json::Value = serde_json::from_str(PLUGIN_DESCRIPTOR_JSON).unwrap();
    assert_eq!(descriptor["provided_capabilities"].as_array().unwrap().len(), 2);
}
