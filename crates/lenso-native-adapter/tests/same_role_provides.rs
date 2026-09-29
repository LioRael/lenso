#![allow(dead_code)]

use lenso_native_adapter::{plugin, provides};

mod management {
    #[macro_export]
    macro_rules! __test_management_provided_same_role {
        () => { r#"{"capability_id":"example.management@1","descriptor_version":"1.0.0","operations":["identity"],"operation_kinds":{},"default_admission":{"queue_capacity":0,"max_concurrency":1},"operation_admissions":{},"event_admission":null,"cross_lane_transfer":false}"# };
    }
    pub use crate::__test_management_lower_object_same_role as __lenso_native_lower_object_same_role;
    pub use crate::__test_management_lower_same_role as __lenso_native_lower_same_role;
    pub use crate::__test_management_provided_same_role as __lenso_provided_same_role;
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
                fn identity(
                    &self,
                ) -> Result<&'static str, __LensoNativeSupportSameRole::RuntimeFailure> {
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
                fn identity(
                    &self,
                ) -> Result<&'static str, __LensoNativeSupportSameRole::RuntimeFailure> {
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
    pub use crate::__test_authentication_lower_object_same_role as __lenso_native_lower_object_same_role;
    pub use crate::__test_authentication_lower_same_role as __lenso_native_lower_same_role;
    pub use crate::__test_authentication_provided_same_role as __lenso_provided_same_role;
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
                fn identity(
                    &self,
                ) -> Result<&'static str, __LensoNativeSupportSameRole::RuntimeFailure> {
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
                fn identity(
                    &self,
                ) -> Result<&'static str, __LensoNativeSupportSameRole::RuntimeFailure> {
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
    // Instance receivers exercise the generated object-provider lowering.
    #[allow(clippy::unused_self)]
    fn management_identity(&self) -> &'static str {
        "management"
    }
    #[allow(clippy::unused_self)]
    fn authentication_identity(&self) -> &'static str {
        "authentication"
    }
}

mod bounded {
    use super::management;
    use lenso_native_adapter::{plugin, provides};

    #[plugin(request_queue_capacity = 16, request_max_concurrency = 2)]
    #[derive(Debug)]
    struct BoundedProvider {}

    #[provides(management::SameRole)]
    impl BoundedProvider {
        #[allow(clippy::unused_self)]
        fn management_identity(&self) -> &'static str {
            "bounded"
        }
    }

    #[test]
    fn root_admission_override_is_retained_in_derived_artifact() {
        let descriptor: serde_json::Value = serde_json::from_str(PLUGIN_DESCRIPTOR_JSON).unwrap();
        assert_eq!(
            descriptor["provided_capabilities"][0]["default_admission"]["queue_capacity"],
            16
        );
        assert_eq!(
            descriptor["provided_capabilities"][0]["default_admission"]["max_concurrency"],
            2
        );
        assert_eq!(
            descriptor["provided_capabilities"][0]["capability_id"],
            "example.management@1"
        );
        let artifact =
            std::str::from_utf8(__LENSO_PLUGIN_DESCRIPTOR_ARTIFACT_BoundedProvider).unwrap();
        assert!(artifact.contains(PLUGIN_DESCRIPTOR_JSON));
        let default: serde_json::Value =
            serde_json::from_str(super::PLUGIN_DESCRIPTOR_JSON).unwrap();
        assert_eq!(
            default["provided_capabilities"][0]["default_admission"]["max_concurrency"],
            1
        );
    }
}

#[test]
fn two_namespaces_with_the_same_role_keep_independent_projections() {
    let provider = Provider {};
    assert_eq!(
        management::SameRoleProvider::identity(&provider).unwrap(),
        "management"
    );
    assert_eq!(
        authentication::SameRoleProvider::identity(&provider).unwrap(),
        "authentication"
    );
    let descriptor: serde_json::Value = serde_json::from_str(PLUGIN_DESCRIPTOR_JSON).unwrap();
    assert_eq!(
        descriptor["provided_capabilities"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
