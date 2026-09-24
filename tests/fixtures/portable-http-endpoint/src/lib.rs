//! Business behavior compiled unchanged into Native, Component, and Workers fixtures.

pub const ROUTES: [(&str, &str, &str); 4] = [
    ("method", "GET", "/method/{item}"),
    ("bytes", "POST", "/bytes"),
    ("reject", "GET", "/reject"),
    ("failure", "GET", "/failure"),
];

/// One outcome of the target-independent business handler.
#[derive(Debug)]
pub enum Reply {
    Bytes(Vec<u8>),
    DomainError,
    RuntimeFailure,
}

/// Handles one already-routed HTTP request without using a host API.
pub fn handle(route_id: &str, method: &str, path: &str, body: &[u8]) -> Reply {
    match (route_id, method, path) {
        ("method", "GET", path) if path.starts_with("/method/") => {
            Reply::Bytes(format!("{method} {path}").into_bytes())
        }
        ("bytes", "POST", "/bytes") => Reply::Bytes(body.to_vec()),
        ("failure", "GET", "/failure") => Reply::RuntimeFailure,
        // The declared reject route and invalid internal dispatches both
        // become intentional domain errors at the HTTP boundary.
        _ => Reply::DomainError,
    }
}

#[cfg(feature = "linked")]
pub mod linked {
    use std::rc::Rc;

    use lenso_capability_http_endpoint::{
        DescribeRequest, DescribeResponse, DescribeResponseRoutesItem, EndpointDescribe,
        EndpointEndpoint, EndpointHandle, EndpointProvider, HandleError, HandleRequest,
        HandleResponse,
    };
    use lenso_kernel::{InvocationContext, NativeRequestFuture, RuntimeFailure};
    use lenso_native_adapter::{
        NativePluginFactory, NativePluginFactoryContext, NativePluginInstance,
    };

    use super::{ROUTES, Reply, handle};

    pub const PACKAGE_ID: &str = "fixture.portable-http";

    #[derive(Debug)]
    struct NativeEndpoint;

    impl EndpointProvider for NativeEndpoint {
        fn describe(
            &self,
            _: InvocationContext,
            _: DescribeRequest,
        ) -> NativeRequestFuture<EndpointDescribe> {
            let routes = ROUTES
                .into_iter()
                .map(|(route_id, method, path)| DescribeResponseRoutesItem {
                    route_id: route_id.to_owned(),
                    method: method.to_owned(),
                    path: path.to_owned(),
                    openapi: None,
                })
                .collect();
            Box::pin(async move { Ok(Ok(DescribeResponse { routes })) })
        }

        fn handle(
            &self,
            _: InvocationContext,
            request: HandleRequest,
        ) -> NativeRequestFuture<EndpointHandle> {
            Box::pin(async move {
                match handle(
                    &request.route_id,
                    &request.method,
                    &request.path,
                    &request.body,
                ) {
                    Reply::Bytes(body) => Ok(Ok(HandleResponse {
                        status: 200,
                        headers: Vec::new(),
                        body: body.into(),
                    })),
                    Reply::DomainError => Ok(Err(HandleError::Rejected)),
                    Reply::RuntimeFailure => Err(RuntimeFailure::PluginFailure {
                        detail: "portable HTTP fixture failure".to_owned(),
                    }),
                }
            })
        }
    }

    #[derive(Debug)]
    pub struct NativeEndpointFactory;

    impl NativePluginFactory for NativeEndpointFactory {
        fn package_id(&self) -> &'static str {
            PACKAGE_ID
        }

        fn package_version(&self) -> &'static str {
            "0.0.0"
        }

        fn instantiate(
            &self,
            _: NativePluginFactoryContext<'_>,
        ) -> Result<NativePluginInstance, RuntimeFailure> {
            Ok(NativePluginInstance::new(vec![Rc::new(
                EndpointEndpoint::new(NativeEndpoint),
            )]))
        }
    }
}
