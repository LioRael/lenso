//! A runnable, native Web authoring golden path.
//!
//! This deliberately drives the Host preset all the way through the real TCP
//! ingress.  It is not a second router: Host selection resolves the App Plan,
//! Ingress performs routing and middleware, the HTTP endpoint extracts a
//! credential-backed actor, the business target authorizes the signed
//! assertion, and the optional `OpenAPI` Plugin validates and serves the typed
//! contract.

use std::{cell::RefCell, collections::BTreeMap, fmt::Write as _, net::SocketAddr, rc::Rc};

use futures::future::LocalBoxFuture;
use http::HeaderValue;
use lenso_app_plan::{
    CapabilityEndpointPlan, CapabilityRequirementPlan,
    authoring::{HostBinding, PluginDescriptor, PluginInstanceId},
};
use lenso_auth_sdk::{
    ActorAssertion, ActorAssertionIssuer, ActorProjectionError, FixedClock, TypedActor, Validity,
    audience, authenticated_response,
};
use lenso_capability_auth::{
    AUTHENTICATE_OPERATION, Auth, AuthClient, AuthEndpoint, AuthProvider,
    CAPABILITY_ID as AUTH_CAPABILITY_ID, DESCRIPTOR_VERSION as AUTH_DESCRIPTOR_VERSION,
};
use lenso_capability_http_endpoint::{
    CAPABILITY_ID as HTTP_CAPABILITY_ID, DESCRIBE_OPERATION,
    DESCRIPTOR_VERSION as HTTP_DESCRIPTOR_VERSION, EndpointEndpoint, EndpointHandleInvocationError,
    ExtractorFuture, FromRequest, HANDLE_OPERATION, HandleRequest, HandleResponse, Json,
    JsonSchema, Path, endpoint,
    response::{self, StatusCode},
};
use lenso_http_auth::{AuthClientSource, AuthenticatedHttpActor, extract_authenticated_actor};
use lenso_kernel::{
    ActivateContext, DeactivateContext, InvocationContext, NativeRequestEndpoint,
    NativeRequestFuture, NativeRequestHandle, PluginDependencies, PluginFuture, PluginLifecycle,
    RequestCapability, RuntimeFailure,
};
use lenso_native_adapter::{NativePluginFactory, NativePluginFactoryContext, NativePluginInstance};
use lenso_openapi_plugin::PACKAGE_ID as OPENAPI_PACKAGE_ID;
use lenso_web_host::{NativeWebHost, WebHostError};
use lenso_web_ingress_plugin::{
    WebIngressMiddleware, WebIngressMiddlewareOutcome, WebIngressRequest, WebIngressResponse,
};
use serde::{Deserialize, Serialize};
use time::{Duration as TimeDuration, OffsetDateTime};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    task::LocalSet,
};

const AUTH_PACKAGE_ID: &str = "fixture.golden-web.auth";
const ENDPOINT_PACKAGE_ID: &str = "fixture.golden-web.orders-http";
const ORDERS_PACKAGE_ID: &str = "fixture.golden-web.orders";
const PACKAGE_VERSION: &str = "0.0.0";
const ORDERS_CAPABILITY_ID: &str = "fixture.golden-web.orders@1";
const ORDERS_DESCRIPTOR_VERSION: &str = "1.0.0";
const READ_ORDER_OPERATION: &str = "read";

#[tokio::test(flavor = "current_thread")]
async fn native_web_host_golden_path_selects_plan_and_uses_real_ingress() {
    LocalSet::new()
        .run_until(async {
            let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
            let issuer = ActorAssertionIssuer::new("golden-web.api-token", b"test-signing-key");
            let observed_actor = Rc::new(RefCell::new(None));
            let host = golden_host(issuer.clone(), now, observed_actor.clone(), true);

            let plan = host.resolve_plan().unwrap();
            assert!(
                plan.plugin_instances()
                    .iter()
                    .any(|instance| instance.package_id() == ENDPOINT_PACKAGE_ID)
            );
            assert!(
                plan.plugin_instances()
                    .iter()
                    .any(|instance| instance.package_id() == OPENAPI_PACKAGE_ID)
            );

            let running = host.start().await.unwrap();
            let address = running.address();
            let manifest = running
                .route_manifest()
                .expect("Ingress publishes a route manifest");
            assert!(manifest.routes().iter().any(|route| {
                route.method == "GET"
                    && route.path == "/orders/{order_id}"
                    && route.route_id == "golden.orders.read"
            }));

            let absent = request(address, "/orders/order-42", &[]).await;
            assert_eq!(absent.status, 401);
            assert_eq!(
                absent.headers.get("content-type").map(String::as_str),
                Some("application/problem+json; charset=utf-8")
            );
            assert_eq!(
                absent.headers.get("www-authenticate").map(String::as_str),
                Some("Bearer")
            );
            assert!(absent.body.contains(r#""code":"authentication_required""#));
            assert!(observed_actor.borrow().is_none());

            let accepted = request(
                address,
                "/orders/order-42",
                &[("Authorization", "Bearer good-token")],
            )
            .await;
            assert_eq!(accepted.status, 200);
            assert_eq!(
                accepted
                    .headers
                    .get("x-lenso-golden-path")
                    .map(String::as_str),
                Some("real-ingress")
            );
            assert_eq!(accepted.body, r#"{"id":"order-42","owner":"user-123"}"#);
            assert_eq!(observed_actor.borrow().as_deref(), Some("user-123"));

            let document = request(address, "/openapi.json", &[]).await;
            assert_eq!(document.status, 200);
            assert_eq!(
                document.headers.get("content-type").map(String::as_str),
                Some("application/json; charset=utf-8")
            );
            let document: serde_json::Value = serde_json::from_str(&document.body).unwrap();
            let contract_operation = &document["paths"]["/contract/orders/{order_id}"]["get"];
            assert_eq!(contract_operation["operationId"], "golden.orders.contract");
            assert!(contract_operation.get("x-lenso-contract").is_none());

            running.shutdown().await.unwrap();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn strict_openapi_contract_compiles_an_equivalent_typed_client_and_serves_its_request() {
    LocalSet::new()
        .run_until(async {
            let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
            let issuer = ActorAssertionIssuer::new("golden-web.api-token", b"test-signing-key");
            let running = golden_host(issuer, now, Rc::new(RefCell::new(None)), true)
                .start()
                .await
                .unwrap();

            // This is a compiled equivalent of generated client code: it accepts
            // only the typed public surface emitted at `/openapi.json`, then uses
            // the resulting method/path/response contract for a real request.
            // It deliberately has no server DTO or direct Endpoint access.
            let document = request(running.address(), "/openapi.json", &[]).await;
            assert_eq!(document.status, 200);
            let client = GeneratedContractOrderClient::compile(
                running.address(),
                &serde_json::from_str(&document.body).unwrap(),
            )
            .unwrap();

            let order = client.read("order-42").await.unwrap();
            assert_eq!(
                order,
                GeneratedContractOrder {
                    id: "order-42".into()
                }
            );

            running.shutdown().await.unwrap();
        })
        .await;
}

#[test]
fn native_web_host_fails_closed_when_the_credential_capability_is_not_released() {
    let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
    let issuer = ActorAssertionIssuer::new("golden-web.api-token", b"test-signing-key");
    let host = golden_host(issuer, now, Rc::new(RefCell::new(None)), false);

    let WebHostError::Plan(detail) = host.resolve_plan().unwrap_err() else {
        panic!("a missing Auth provider must fail while resolving the App Plan");
    };
    assert!(detail.contains(AUTH_CAPABILITY_ID), "{detail}");
}

fn golden_host(
    issuer: ActorAssertionIssuer,
    now: OffsetDateTime,
    observed_actor: Rc<RefCell<Option<String>>>,
    include_auth: bool,
) -> NativeWebHost {
    let endpoint = instance_id(ENDPOINT_PACKAGE_ID);
    let orders = instance_id(ORDERS_PACKAGE_ID);
    let mut host = NativeWebHost::new()
        .bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .with_middleware(GoldenResponseMarker)
        .factory(
            GoldenOrdersFactory {
                verifier: issuer.verifier(),
                now,
                observed_actor,
            },
            orders_descriptor(),
        )
        .factory(GoldenOrdersHttpFactory::default(), endpoint_descriptor())
        .enable(OPENAPI_PACKAGE_ID)
        .with_binding(HostBinding::to_instance(
            endpoint.clone(),
            ORDERS_CAPABILITY_ID,
            orders,
        ))
        .with_binding(HostBinding::to_instances(
            instance_id(OPENAPI_PACKAGE_ID),
            HTTP_CAPABILITY_ID,
            [endpoint.clone()],
        ));
    if include_auth {
        host = host
            .factory(TokenAuthFactory { issuer, now }, auth_descriptor())
            .with_binding(HostBinding::to_instance(
                endpoint,
                AUTH_CAPABILITY_ID,
                instance_id(AUTH_PACKAGE_ID),
            ));
    }
    host
}

fn instance_id(package_id: &str) -> PluginInstanceId {
    PluginInstanceId::new(package_id, "default")
}

fn auth_descriptor() -> PluginDescriptor {
    PluginDescriptor::new(AUTH_PACKAGE_ID, PACKAGE_VERSION, "golden-web-auth").with_capability(
        CapabilityEndpointPlan::new(
            AUTH_CAPABILITY_ID,
            AUTH_DESCRIPTOR_VERSION,
            [AUTHENTICATE_OPERATION],
        ),
    )
}

fn orders_descriptor() -> PluginDescriptor {
    PluginDescriptor::new(ORDERS_PACKAGE_ID, PACKAGE_VERSION, "golden-web-orders").with_capability(
        CapabilityEndpointPlan::new(
            ORDERS_CAPABILITY_ID,
            ORDERS_DESCRIPTOR_VERSION,
            [READ_ORDER_OPERATION],
        ),
    )
}

fn endpoint_descriptor() -> PluginDescriptor {
    PluginDescriptor::new(ENDPOINT_PACKAGE_ID, PACKAGE_VERSION, "http-endpoints")
        .with_capability(CapabilityEndpointPlan::new(
            HTTP_CAPABILITY_ID,
            HTTP_DESCRIPTOR_VERSION,
            [DESCRIBE_OPERATION, HANDLE_OPERATION],
        ))
        .with_requirement(CapabilityRequirementPlan::one(
            AUTH_CAPABILITY_ID,
            AUTH_DESCRIPTOR_VERSION,
        ))
        .with_requirement(CapabilityRequirementPlan::one(
            ORDERS_CAPABILITY_ID,
            ORDERS_DESCRIPTOR_VERSION,
        ))
}

#[derive(Debug)]
struct GoldenResponseMarker;

impl WebIngressMiddleware for GoldenResponseMarker {
    fn identity(&self) -> &'static str {
        "fixture.golden-web.response-marker"
    }

    fn before_request<'a>(
        &'a self,
        _request: &'a mut WebIngressRequest,
    ) -> LocalBoxFuture<'a, Result<WebIngressMiddlewareOutcome, RuntimeFailure>> {
        Box::pin(async { Ok(WebIngressMiddlewareOutcome::Continue) })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a WebIngressRequest,
        response: &'a mut WebIngressResponse,
    ) -> LocalBoxFuture<'a, Result<(), RuntimeFailure>> {
        Box::pin(async move {
            response.headers_mut().insert(
                "x-lenso-golden-path",
                HeaderValue::from_static("real-ingress"),
            );
            Ok(())
        })
    }
}

#[derive(Clone, Debug)]
struct TokenAuthFactory {
    issuer: ActorAssertionIssuer,
    now: OffsetDateTime,
}

impl NativePluginFactory for TokenAuthFactory {
    fn package_id(&self) -> &'static str {
        AUTH_PACKAGE_ID
    }

    fn package_version(&self) -> &'static str {
        PACKAGE_VERSION
    }

    fn instantiate(
        &self,
        _context: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, RuntimeFailure> {
        Ok(NativePluginInstance::new(vec![Rc::new(AuthEndpoint::new(
            TokenAuth {
                issuer: self.issuer.clone(),
                now: self.now,
            },
        ))]))
    }
}

#[derive(Debug)]
struct TokenAuth {
    issuer: ActorAssertionIssuer,
    now: OffsetDateTime,
}

impl AuthProvider for TokenAuth {
    fn authenticate(
        &self,
        _context: InvocationContext,
        request: lenso_capability_auth::AuthenticateRequest,
    ) -> NativeRequestFuture<Auth> {
        let outcome = match request.credential {
            None => Ok(Ok(lenso_auth_sdk::absent_response())),
            Some(credential)
                if credential.scheme == "bearer" && credential.value == "good-token" =>
            {
                let assertion = self.issuer.issue(
                    "user-123",
                    "user",
                    "api-token",
                    [audience(ORDERS_CAPABILITY_ID, READ_ORDER_OPERATION)],
                    Validity::new(
                        self.now - TimeDuration::seconds(1),
                        self.now + TimeDuration::minutes(1),
                    )
                    .unwrap(),
                    BTreeMap::new(),
                );
                Ok(Ok(authenticated_response(&assertion)))
            }
            Some(_) => Ok(Err(lenso_capability_auth::AuthenticateError::Invalid)),
        };
        Box::pin(futures::future::ready(outcome))
    }
}

#[derive(Clone, Debug, Default)]
struct GoldenOrdersHttpFactory {
    dependencies: Rc<RefCell<Option<EndpointDependencies>>>,
}

impl NativePluginFactory for GoldenOrdersHttpFactory {
    fn package_id(&self) -> &'static str {
        ENDPOINT_PACKAGE_ID
    }

    fn package_version(&self) -> &'static str {
        PACKAGE_VERSION
    }

    fn instantiate(
        &self,
        _context: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, RuntimeFailure> {
        Ok(NativePluginInstance::with_lifecycle(
            vec![Rc::new(EndpointEndpoint::new(GoldenOrdersHttp {
                dependencies: self.dependencies.clone(),
            }))],
            EndpointLifecycle {
                dependencies: self.dependencies.clone(),
            },
        ))
    }
}

#[derive(Clone, Debug)]
struct EndpointDependencies {
    auth: Rc<AuthClient>,
    orders: Rc<OrdersClient>,
}

#[derive(Debug)]
struct EndpointLifecycle {
    dependencies: Rc<RefCell<Option<EndpointDependencies>>>,
}

impl PluginLifecycle for EndpointLifecycle {
    fn activate(&self, context: ActivateContext) -> PluginFuture {
        let result = (|| {
            self.dependencies
                .borrow_mut()
                .replace(EndpointDependencies {
                    auth: Rc::new(AuthClient::from_dependencies(context.dependencies())?),
                    orders: Rc::new(OrdersClient::from_dependencies(context.dependencies())?),
                });
            Ok(())
        })();
        Box::pin(futures::future::ready(result))
    }

    fn deactivate(&self, _context: DeactivateContext) -> PluginFuture {
        self.dependencies.borrow_mut().take();
        Box::pin(futures::future::ready(Ok(())))
    }
}

#[derive(Clone, Debug)]
struct GoldenOrdersHttp {
    dependencies: Rc<RefCell<Option<EndpointDependencies>>>,
}

#[endpoint(standalone)]
impl GoldenOrdersHttp {
    #[get("golden.orders.read", "/orders/{order_id}")]
    #[openapi({ summary: "Read an authenticated order" })]
    async fn read(
        &self,
        _actor: UserActor,
        context: InvocationContext,
        Path(path): Path<OrderPath>,
    ) -> Result<HandleResponse, EndpointHandleInvocationError> {
        let dependencies = self.dependencies()?;
        let Ok(order) = dependencies
            .orders
            .read(
                context,
                ReadOrderRequest {
                    order_id: path.order_id,
                },
            )
            .await
            .map_err(EndpointHandleInvocationError::Runtime)?
        else {
            return Ok(response::problem(
                StatusCode::FORBIDDEN,
                "insufficient_permission",
                "The authenticated actor cannot read this order.",
            ));
        };
        Ok(response::json(StatusCode::OK, &order)?)
    }

    #[get("golden.orders.contract", "/contract/orders/{order_id}")]
    #[openapi({
        summary: "Read a typed order contract",
        parameters: [{
            name: "order_id",
            in: "path",
            required: true,
            schema: { "type": "string" }
        }],
        responses: {
            "200": {
                description: "Order",
                content: {
                    "application/json": {
                        schema: {
                            "type": "object",
                            required: ["id"],
                            properties: { id: { "type": "string" } }
                        }
                    }
                }
            },
            "400": {
                description: "Invalid path parameter",
                content: {
                    "application/problem+json": {
                        schema: {
                            "type": "object",
                            required: ["type", "title", "status", "detail", "code"],
                            properties: {
                                "type": { "type": "string" },
                                title: { "type": "string" },
                                status: { "const": 400 },
                                detail: { "type": "string" },
                                code: {
                                    "type": "string",
                                    "enum": ["invalid_path_parameters"]
                                }
                            },
                            additionalProperties: false
                        }
                    }
                }
            }
        }
    })]
    #[openapi_contract]
    async fn contract(
        &self,
        Path(path): Path<OrderPath>,
    ) -> Result<Json<ContractOrder>, EndpointHandleInvocationError> {
        futures::future::ready(()).await;
        Ok(Json(ContractOrder { id: path.order_id }))
    }
}

#[derive(Debug)]
pub struct UserActor {
    pub subject: String,
}

impl AuthenticatedHttpActor for UserActor {
    const KIND: &'static str = "user";

    fn from_assertion(assertion: &ActorAssertion) -> Self {
        Self {
            subject: assertion.subject().to_owned(),
        }
    }
}

impl FromRequest<GoldenOrdersHttp> for UserActor {
    fn from_request<'a>(
        provider: &'a GoldenOrdersHttp,
        context: &'a mut InvocationContext,
        request: &'a HandleRequest,
    ) -> ExtractorFuture<'a, Self> {
        extract_authenticated_actor(provider, context, request)
    }
}

impl AuthClientSource for GoldenOrdersHttp {
    fn auth_client(&self) -> Result<Rc<AuthClient>, EndpointHandleInvocationError> {
        Ok(self.dependencies()?.auth)
    }
}

impl GoldenOrdersHttp {
    fn dependencies(&self) -> Result<EndpointDependencies, EndpointHandleInvocationError> {
        self.dependencies.borrow().clone().ok_or({
            EndpointHandleInvocationError::Runtime(RuntimeFailure::Unavailable {
                capability: AUTH_CAPABILITY_ID,
            })
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct OrderPath {
    order_id: String,
}

#[derive(Debug)]
struct ReadOrder;

impl RequestCapability for ReadOrder {
    type Request = ReadOrderRequest;
    type Response = ReadOrderResponse;
    type DomainError = ReadOrderError;
    const ID: &'static str = ORDERS_CAPABILITY_ID;
    const DESCRIPTOR_VERSION: &'static str = ORDERS_DESCRIPTOR_VERSION;
}

#[derive(Debug)]
struct OrdersClient {
    handle: NativeRequestHandle<ReadOrder>,
}

impl OrdersClient {
    fn from_dependencies(dependencies: &PluginDependencies) -> Result<Self, RuntimeFailure> {
        Ok(Self {
            handle: dependencies.one::<ReadOrder>()?,
        })
    }

    async fn read(
        &self,
        context: InvocationContext,
        request: ReadOrderRequest,
    ) -> Result<Result<ReadOrderResponse, ReadOrderError>, RuntimeFailure> {
        self.handle
            .invoke_with_context(READ_ORDER_OPERATION, context, request)
            .await
    }
}

#[derive(Debug)]
struct ReadOrderRequest {
    order_id: String,
}

#[derive(Debug, Serialize)]
struct ReadOrderResponse {
    id: String,
    owner: String,
}

#[derive(Debug)]
struct ReadOrderError;

#[derive(Clone, Debug)]
struct GoldenOrdersFactory {
    verifier: lenso_auth_sdk::ActorAssertionVerifier,
    now: OffsetDateTime,
    observed_actor: Rc<RefCell<Option<String>>>,
}

impl NativePluginFactory for GoldenOrdersFactory {
    fn package_id(&self) -> &'static str {
        ORDERS_PACKAGE_ID
    }

    fn package_version(&self) -> &'static str {
        PACKAGE_VERSION
    }

    fn instantiate(
        &self,
        _context: NativePluginFactoryContext<'_>,
    ) -> Result<NativePluginInstance, RuntimeFailure> {
        Ok(NativePluginInstance::new(vec![Rc::new(OrdersEndpoint {
            verifier: self.verifier.clone(),
            now: self.now,
            observed_actor: self.observed_actor.clone(),
        })]))
    }
}

#[derive(Debug)]
struct OrdersEndpoint {
    verifier: lenso_auth_sdk::ActorAssertionVerifier,
    now: OffsetDateTime,
    observed_actor: Rc<RefCell<Option<String>>>,
}

impl NativeRequestEndpoint for OrdersEndpoint {
    fn capability_id(&self) -> &'static str {
        ORDERS_CAPABILITY_ID
    }

    fn descriptor_version(&self) -> &'static str {
        ORDERS_DESCRIPTOR_VERSION
    }

    fn operations(&self) -> &'static [&'static str] {
        &[READ_ORDER_OPERATION]
    }

    fn invoke(
        &self,
        operation: &str,
        request: Box<dyn std::any::Any>,
        context: InvocationContext,
    ) -> futures::future::LocalBoxFuture<
        'static,
        Result<Result<Box<dyn std::any::Any>, Box<dyn std::any::Any>>, RuntimeFailure>,
    > {
        if operation != READ_ORDER_OPERATION {
            return Box::pin(futures::future::ready(Err(
                RuntimeFailure::UnknownOperation {
                    capability: ORDERS_CAPABILITY_ID,
                    operation: operation.to_owned(),
                },
            )));
        }
        let Ok(request) = request.downcast::<ReadOrderRequest>() else {
            return Box::pin(futures::future::ready(Err(
                RuntimeFailure::ProtocolViolation {
                    capability: ORDERS_CAPABILITY_ID,
                },
            )));
        };
        let result = self
            .verifier
            .project_context::<OrdersActor>(
                &context,
                ORDERS_CAPABILITY_ID,
                READ_ORDER_OPERATION,
                &FixedClock::new(self.now),
            )
            .map(|actor| {
                self.observed_actor
                    .borrow_mut()
                    .replace(actor.user_id.clone());
                Box::new(ReadOrderResponse {
                    id: request.order_id,
                    owner: actor.user_id,
                }) as Box<dyn std::any::Any>
            })
            .map_err(|_| Box::new(ReadOrderError) as Box<dyn std::any::Any>);
        Box::pin(futures::future::ready(Ok(result)))
    }
}

#[derive(Debug)]
struct OrdersActor {
    user_id: String,
}

impl TypedActor for OrdersActor {
    fn from_assertion(assertion: &ActorAssertion) -> Result<Self, ActorProjectionError> {
        if assertion.actor_kind() != "user" {
            return Err(ActorProjectionError::UnexpectedActorKind {
                expected: "user".to_owned(),
                actual: assertion.actor_kind().to_owned(),
            });
        }
        Ok(Self {
            user_id: assertion.subject().to_owned(),
        })
    }
}

#[derive(JsonSchema, Serialize)]
struct ContractOrder {
    id: String,
}

struct HttpResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: String,
}

/// Test-only output of an ordinary `OpenAPI` client generator for the strict
/// `golden.orders.contract` operation.
///
/// The server's `ContractOrder` remains the sole source DTO. This client-side
/// representation exists only to prove that the served public document can be
/// compiled into a separately typed consumer and used against real ingress.
#[derive(Debug, Deserialize, Eq, PartialEq)]
struct GeneratedContractOrder {
    id: String,
}

/// A minimal compiled representation of the `OpenAPI` operation a client
/// generator would emit. `compile` fails closed if the served document stops
/// describing a request that this type can safely make.
#[derive(Debug)]
struct GeneratedContractOrderClient {
    address: SocketAddr,
    path: String,
}

impl GeneratedContractOrderClient {
    fn compile(address: SocketAddr, document: &serde_json::Value) -> Result<Self, String> {
        if document.get("openapi").and_then(serde_json::Value::as_str) != Some("3.1.0") {
            return Err("generated client requires an OpenAPI 3.1 document".to_owned());
        }
        let path = "/contract/orders/{order_id}";
        let operation = document
            .get("paths")
            .and_then(serde_json::Value::as_object)
            .and_then(|paths| paths.get(path))
            .and_then(serde_json::Value::as_object)
            .and_then(|methods| methods.get("get"))
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| {
                "generated client requires GET /contract/orders/{order_id}".to_owned()
            })?;
        if operation
            .get("operationId")
            .and_then(serde_json::Value::as_str)
            != Some("golden.orders.contract")
        {
            return Err("generated client operationId drifted".to_owned());
        }
        validate_generated_path_parameter(operation)?;
        validate_generated_success_response(operation)?;
        validate_generated_problem_response(operation)?;
        Ok(Self {
            address,
            path: path.to_owned(),
        })
    }

    async fn read(&self, order_id: &str) -> Result<GeneratedContractOrder, String> {
        if order_id.is_empty() || order_id.contains(['/', '{', '}']) {
            return Err("generated path parameter is invalid".to_owned());
        }
        let path = self.path.replace("{order_id}", order_id);
        let response = request(self.address, &path, &[]).await;
        if response.status != 200 {
            return Err(format!(
                "generated client expected 200, received {}: {}",
                response.status, response.body
            ));
        }
        if response.headers.get("content-type").map(String::as_str)
            != Some("application/json; charset=utf-8")
        {
            return Err("generated client expected an application/json response".to_owned());
        }
        serde_json::from_str(&response.body)
            .map_err(|error| format!("generated client could not decode ContractOrder: {error}"))
    }
}

fn validate_generated_path_parameter(
    operation: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let parameters = operation
        .get("parameters")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "generated client requires path parameters".to_owned())?;
    let [parameter] = parameters.as_slice() else {
        return Err("generated client requires exactly one path parameter".to_owned());
    };
    if parameter.get("name").and_then(serde_json::Value::as_str) != Some("order_id")
        || parameter.get("in").and_then(serde_json::Value::as_str) != Some("path")
        || parameter
            .get("required")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || parameter
            .pointer("/schema/type")
            .and_then(serde_json::Value::as_str)
            != Some("string")
    {
        return Err("generated client path parameter contract drifted".to_owned());
    }
    Ok(())
}

fn validate_generated_success_response(
    operation: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let operation = serde_json::Value::Object(operation.clone());
    let schema = operation
        .pointer("/responses/200/content/application~1json/schema")
        .ok_or_else(|| "generated client requires a JSON 200 response".to_owned())?;
    let required_id = schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|fields| fields.iter().any(|field| field == "id"));
    if schema.get("type").and_then(serde_json::Value::as_str) != Some("object")
        || !required_id
        || schema
            .pointer("/properties/id/type")
            .and_then(serde_json::Value::as_str)
            != Some("string")
    {
        return Err("generated client success response contract drifted".to_owned());
    }
    Ok(())
}

fn validate_generated_problem_response(
    operation: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let operation = serde_json::Value::Object(operation.clone());
    let schema = operation
        .pointer("/responses/400/content/application~1problem+json/schema")
        .ok_or_else(|| "generated client requires a problem+json 400 response".to_owned())?;
    let required = schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "generated client requires a typed problem response".to_owned())?;
    let required_fields = ["type", "title", "status", "detail", "code"];
    if !required_fields
        .iter()
        .all(|field| required.iter().any(|actual| actual == field))
        || schema
            .pointer("/properties/status/const")
            .and_then(serde_json::Value::as_u64)
            != Some(400)
        || schema
            .pointer("/properties/code/enum")
            .and_then(serde_json::Value::as_array)
            .is_none_or(|codes| codes.iter().all(|code| code != "invalid_path_parameters"))
    {
        return Err("generated client problem response contract drifted".to_owned());
    }
    Ok(())
}

async fn request(address: SocketAddr, path: &str, headers: &[(&str, &str)]) -> HttpResponse {
    let mut stream = TcpStream::connect(address).await.unwrap();
    let headers = headers
        .iter()
        .fold(String::new(), |mut wire, (name, value)| {
            write!(wire, "{name}: {value}\r\n").unwrap();
            wire
        });
    let wire = format!(
        "GET {path} HTTP/1.1\r\nHost: {address}\r\n{headers}Content-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(wire.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response = String::from_utf8(response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    HttpResponse {
        status: head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        headers: head
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_owned()))
            .collect(),
        body: body.to_owned(),
    }
}
