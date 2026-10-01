use lenso_capability_http_endpoint::prelude::*;
use lenso_kernel::InvocationContext;
use std::{cell::RefCell, rc::Rc};

#[lenso::plugin(id = "example.engine-web", root_slot = "web")]
#[derive(Clone, Debug, Default)]
pub struct Plugin {
    events: Rc<RefCell<Vec<&'static str>>>,
}

// Middleware uses the async Endpoint API even when this example needs no I/O.
#[allow(unknown_lints, clippy::unused_async, clippy::unused_async_trait_impl)]
impl Plugin {
    async fn global(
        &self,
        context: InvocationContext,
        request: HandleRequest,
    ) -> Result<MiddlewareOutcome, EndpointHandleInvocationError> {
        self.events.borrow_mut().push("global");
        Ok(MiddlewareOutcome::next(context, request))
    }
    async fn scoped(
        &self,
        context: InvocationContext,
        request: HandleRequest,
    ) -> Result<MiddlewareOutcome, EndpointHandleInvocationError> {
        self.events.borrow_mut().push("scope");
        Ok(MiddlewareOutcome::next(context, request))
    }
    async fn local(
        &self,
        context: InvocationContext,
        request: HandleRequest,
    ) -> Result<MiddlewareOutcome, EndpointHandleInvocationError> {
        self.events.borrow_mut().push("local");
        if request
            .headers
            .iter()
            .any(|header| header.name == "x-reject")
        {
            return Ok(MiddlewareOutcome::response(
                lenso_capability_http_endpoint::response::text(StatusCode::FORBIDDEN, "denied"),
            ));
        }
        Ok(MiddlewareOutcome::next(context, request))
    }
}

#[derive(Debug, serde::Deserialize)]
struct ItemPath {
    id: String,
}

struct Observed;
impl lenso_capability_http_endpoint::FromRequest<Plugin> for Observed {
    fn from_request<'a>(
        provider: &'a Plugin,
        _: &'a mut InvocationContext,
        _: &'a HandleRequest,
    ) -> lenso_capability_http_endpoint::ExtractorFuture<'a, Self> {
        Box::pin(async move {
            provider.events.borrow_mut().push("extractor");
            Ok(Self)
        })
    }
}

include!(concat!(env!("OUT_DIR"), "/web_routes.rs"));

#[cfg(test)]
mod tests {
    #[test]
    fn both_styles_route_through_the_existing_simulated_host() {
        use bytes::Bytes;
        use http::Request;
        use lenso_kernel::{DeterministicDriver, Kernel, ShutdownOutcome};
        let (plan, registry, web) = lenso_web_host::NativeWebHost::new()
            .plugin::<super::Plugin>()
            .prepare_simulated()
            .unwrap()
            .into_parts();
        let driver = DeterministicDriver::new();
        let app = driver
            .run(Kernel::start_native(plan, driver.clone(), registry))
            .unwrap();
        for path in ["/items/example", "/files/example"] {
            let response = driver
                .run(
                    web.request(
                        Request::builder()
                            .method("GET")
                            .uri(path)
                            .body(Bytes::new())
                            .unwrap(),
                    ),
                )
                .unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(response.body().as_ref(), b"\"example\"");
        }
        assert_eq!(
            driver.run(app.shutdown(std::time::Duration::from_secs(1))),
            ShutdownOutcome::Clean
        );
    }

    #[test]
    fn generated_routes_compile_against_the_official_endpoint_macro() {
        use lenso_capability_http_endpoint::HttpEndpoint;
        assert_eq!(super::Plugin::ROUTES.len(), 4);
        assert_eq!(super::Plugin::ROUTES[1].path(), "/items/{id}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn both_styles_use_typed_extraction_and_the_same_middleware_dispatch() {
        use super::*;
        use lenso_capability_http_endpoint::testing::EndpointTest;
        let plugin = Plugin::default();
        let test = EndpointTest::new(plugin.clone());
        let explicit = test
            .request("item")
            .path_parameter("id", "one")
            .send()
            .await
            .unwrap();
        assert_eq!(explicit.json::<String>().unwrap(), "one");
        assert_eq!(*plugin.events.borrow(), ["global"]);
        plugin.events.borrow_mut().clear();
        let filesystem = test
            .request("files.read")
            .path_parameter("id", "two")
            .send()
            .await
            .unwrap();
        assert_eq!(filesystem.json::<String>().unwrap(), "two");
        assert_eq!(
            *plugin.events.borrow(),
            ["global", "scope", "local", "extractor", "handler"]
        );
        plugin.events.borrow_mut().clear();
        let rejected = test
            .request("files.read")
            .header("x-reject", "yes")
            .send()
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
        assert_eq!(*plugin.events.borrow(), ["global", "scope", "local"]);
        plugin.events.borrow_mut().clear();
        let malformed = test.request("files.read").send().await.unwrap();
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert_eq!(*plugin.events.borrow(), ["global", "scope", "local"]);
    }
}
