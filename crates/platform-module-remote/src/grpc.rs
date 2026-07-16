use crate::config::RemoteModuleConfig;
use crate::protocol::{
    RemoteActionInvokeResponse, RemoteAdminActionInvokeRequest, RemoteAdminGetRequest,
    RemoteAdminListRequest, RemoteAdminQueryRequest, RemoteEventHandleRequest,
    RemoteEventHandleResponse, RemoteFunctionInvokeRequest, RemoteFunctionInvokeResponse,
    RemoteGetResponse, RemoteHttpProxyInvokeRequest, RemoteHttpProxyInvokeResponse,
    RemoteListResponse, RemoteManifestResponse, RemoteQueryResponse,
};
use platform_core::{AppError, AppResult, ErrorCode};
use platform_module::AdminListQuery;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::time::Duration;
use tonic::codegen::GrpcMethod;
use tonic::codegen::http::uri::PathAndQuery;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request, Status};

const GET_MANIFEST_PATH: &str = "/lenso.remote.v1.RemoteModule/GetManifest";
const LIST_ADMIN_RECORDS_PATH: &str = "/lenso.remote.v1.RemoteModule/ListAdminRecords";
const GET_ADMIN_RECORD_PATH: &str = "/lenso.remote.v1.RemoteModule/GetAdminRecord";
const INVOKE_ADMIN_ACTION_PATH: &str = "/lenso.remote.v1.RemoteModule/InvokeAdminAction";
const QUERY_ADMIN_VALUE_PATH: &str = "/lenso.remote.v1.RemoteModule/QueryAdminValue";
const PROXY_HTTP_ROUTE_PATH: &str = "/lenso.remote.v1.RemoteModule/ProxyHttpRoute";
const INVOKE_FUNCTION_PATH: &str = "/lenso.remote.v1.RemoteModule/InvokeFunction";
const HANDLE_EVENT_PATH: &str = "/lenso.remote.v1.RemoteModule/HandleEvent";
const MAX_GRPC_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, PartialEq, prost::Message)]
struct JsonEnvelope {
    // The first gRPC lane reuses stable JSON envelopes; typed proto can replace this later.
    #[prost(string, tag = "1")]
    payload_json: String,
}

pub(crate) async fn fetch_manifest(
    config: &RemoteModuleConfig,
) -> AppResult<RemoteManifestResponse> {
    unary_json(
        config,
        GET_MANIFEST_PATH,
        "manifest",
        &serde_json::json!({}),
    )
    .await
}

pub(crate) async fn list_admin_records(
    config: &RemoteModuleConfig,
    entity: &str,
    query: &AdminListQuery,
) -> AppResult<RemoteListResponse> {
    unary_json(
        config,
        LIST_ADMIN_RECORDS_PATH,
        "admin data list",
        &RemoteAdminListRequest {
            entity: entity.to_owned(),
            limit: query.limit,
            cursor: query.cursor.clone(),
        },
    )
    .await
}

pub(crate) async fn get_admin_record(
    config: &RemoteModuleConfig,
    entity: &str,
    id: &str,
) -> AppResult<RemoteGetResponse> {
    unary_json(
        config,
        GET_ADMIN_RECORD_PATH,
        "admin data get",
        &RemoteAdminGetRequest {
            entity: entity.to_owned(),
            id: id.to_owned(),
        },
    )
    .await
}

pub(crate) async fn invoke_admin_action(
    config: &RemoteModuleConfig,
    action: &str,
    input: serde_json::Value,
) -> AppResult<RemoteActionInvokeResponse> {
    unary_json(
        config,
        INVOKE_ADMIN_ACTION_PATH,
        "admin action invoke",
        &RemoteAdminActionInvokeRequest {
            action: action.to_owned(),
            input,
        },
    )
    .await
}

pub(crate) async fn query_admin_value(
    config: &RemoteModuleConfig,
    query: &str,
) -> AppResult<RemoteQueryResponse> {
    unary_json(
        config,
        QUERY_ADMIN_VALUE_PATH,
        "admin query",
        &RemoteAdminQueryRequest {
            query: query.to_owned(),
        },
    )
    .await
}

pub(crate) async fn proxy_http_route(
    config: &RemoteModuleConfig,
    request: &RemoteHttpProxyInvokeRequest,
) -> AppResult<RemoteHttpProxyInvokeResponse> {
    unary_json(config, PROXY_HTTP_ROUTE_PATH, "HTTP proxy", request).await
}

pub(crate) async fn invoke_function(
    config: &RemoteModuleConfig,
    request: &RemoteFunctionInvokeRequest,
) -> AppResult<RemoteFunctionInvokeResponse> {
    unary_json(
        config,
        INVOKE_FUNCTION_PATH,
        "runtime function invoke",
        request,
    )
    .await
}

pub(crate) async fn handle_event(
    config: &RemoteModuleConfig,
    request: &RemoteEventHandleRequest,
) -> AppResult<RemoteEventHandleResponse> {
    unary_json(config, HANDLE_EVENT_PATH, "event handler invoke", request).await
}

async fn unary_json<TRequest, TResponse>(
    config: &RemoteModuleConfig,
    path: &'static str,
    operation: &'static str,
    request: &TRequest,
) -> AppResult<TResponse>
where
    TRequest: Serialize,
    TResponse: DeserializeOwned,
{
    let mut client = connect(config, operation).await?;
    let mut request = Request::new(JsonEnvelope {
        payload_json: serde_json::to_string(request).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("remote {operation} gRPC request could not be encoded: {error}"),
            )
        })?,
    });
    request.set_timeout(Duration::from_millis(config.timeout_ms));
    apply_auth(&mut request, config.auth_token.as_deref(), operation)?;

    client.ready().await.map_err(|error| {
        status_error(
            Status::unknown(format!("remote gRPC service was not ready: {}", error)),
            operation,
        )
    })?;
    request.extensions_mut().insert(GrpcMethod::new(
        "lenso.remote.v1.RemoteModule",
        method_name(path),
    ));
    let codec = tonic_prost::ProstCodec::<JsonEnvelope, JsonEnvelope>::default();
    let response = client
        .unary(request, PathAndQuery::from_static(path), codec)
        .await
        .map_err(|status| status_error(status, operation))?
        .into_inner();

    serde_json::from_str(&response.payload_json).map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("remote {operation} gRPC response was invalid JSON: {error}"),
        )
    })
}

async fn connect(
    config: &RemoteModuleConfig,
    operation: &'static str,
) -> AppResult<tonic::client::Grpc<Channel>> {
    let timeout = Duration::from_millis(config.timeout_ms);
    let endpoint = Endpoint::new(config.base_url.clone())
        .map_err(|error| {
            AppError::new(
                ErrorCode::Validation,
                format!(
                    "remote {operation} gRPC endpoint was invalid: {} ({})",
                    config.base_url, error
                ),
            )
        })?
        .connect_timeout(timeout)
        .timeout(timeout);

    let channel = endpoint.connect().await.map_err(|error| {
        AppError::new(
            ErrorCode::ExternalDependency,
            format!("remote {operation} gRPC connection failed: {error}"),
        )
        .retryable()
    })?;

    Ok(tonic::client::Grpc::new(channel)
        .max_decoding_message_size(MAX_GRPC_MESSAGE_BYTES)
        .max_encoding_message_size(MAX_GRPC_MESSAGE_BYTES))
}

fn apply_auth<T>(
    request: &mut Request<T>,
    token: Option<&str>,
    operation: &'static str,
) -> AppResult<()> {
    let Some(token) = token else {
        return Ok(());
    };
    let value = MetadataValue::try_from(format!("Bearer {token}").as_str()).map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("remote {operation} gRPC auth metadata was invalid: {error}"),
        )
    })?;
    request.metadata_mut().insert("authorization", value);
    Ok(())
}

fn status_error(status: Status, operation: &'static str) -> AppError {
    let code = status.code();
    let mut error = AppError::new(
        error_code_from_status(code),
        format!("remote {operation} gRPC failed: {}", status.message()),
    );
    if status_is_retryable(code) {
        error = error.retryable();
    }
    error
}

fn error_code_from_status(code: Code) -> ErrorCode {
    match code {
        Code::InvalidArgument | Code::FailedPrecondition | Code::OutOfRange => {
            ErrorCode::Validation
        }
        Code::Unauthenticated => ErrorCode::Unauthorized,
        Code::PermissionDenied => ErrorCode::Forbidden,
        Code::NotFound => ErrorCode::NotFound,
        Code::AlreadyExists | Code::Aborted => ErrorCode::Conflict,
        Code::ResourceExhausted => ErrorCode::RateLimited,
        _ => ErrorCode::ExternalDependency,
    }
}

fn status_is_retryable(code: Code) -> bool {
    matches!(
        code,
        Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted | Code::Unknown
    )
}

fn method_name(path: &str) -> &'static str {
    match path {
        GET_MANIFEST_PATH => "GetManifest",
        LIST_ADMIN_RECORDS_PATH => "ListAdminRecords",
        GET_ADMIN_RECORD_PATH => "GetAdminRecord",
        INVOKE_ADMIN_ACTION_PATH => "InvokeAdminAction",
        QUERY_ADMIN_VALUE_PATH => "QueryAdminValue",
        PROXY_HTTP_ROUTE_PATH => "ProxyHttpRoute",
        INVOKE_FUNCTION_PATH => "InvokeFunction",
        HANDLE_EVENT_PATH => "HandleEvent",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        RemoteAdminActionInvokeRequest, RemoteAdminGetRequest, RemoteAdminListRequest,
        RemoteErrorEnvelope, RemoteEventResultAction, RemoteFunctionInvokeResponse,
        RemoteHttpProxyInvokeRequest,
    };
    use crate::{RemoteModuleSource, RemoteModuleTransport, RemoteRuntimeFunction};
    use platform_core::{ActorContext, CorrelationId, ExecutionContext, ExecutionId, TraceContext};
    use platform_module::{
        AdminAction, AdminActionDangerLevel, AdminDeclarativeSurface, AdminListQuery, AdminSchema,
        EntitySchema, EventHandlerDeclaration, EventSurface, FieldSchema, FieldType,
        ModuleManifest, RuntimeFunctionDeclaration, RuntimeRetryPolicyDeclaration, RuntimeSurface,
    };
    use serde_json::json;
    use std::convert::Infallible;
    use std::net::TcpListener;
    use tokio::time::{Duration as TokioDuration, sleep};
    use tonic::codegen::{Body, BoxFuture, Service, StdError, http};
    use tonic::server::{NamedService, UnaryService};

    #[test]
    fn retryable_grpc_status_maps_to_external_dependency() {
        let error = status_error(Status::unavailable("offline"), "manifest");

        assert_eq!(error.code, ErrorCode::ExternalDependency);
        assert!(error.retryable);
    }

    #[test]
    fn validation_grpc_status_is_not_retryable() {
        let error = status_error(Status::invalid_argument("bad input"), "manifest");

        assert_eq!(error.code, ErrorCode::Validation);
        assert!(!error.retryable);
    }

    #[test]
    fn json_envelope_matches_proto_wire_contract() {
        let envelope = RemoteErrorEnvelope {
            error: crate::protocol::RemoteErrorBody {
                code: "external_dependency_failure".to_owned(),
                message: "remote failed".to_owned(),
                retryable: true,
                details: Vec::new(),
            },
        };
        let request = JsonEnvelope {
            payload_json: serde_json::to_string(&envelope).expect("envelope serializes"),
        };
        let encoded = prost::Message::encode_to_vec(&request);
        let request =
            <JsonEnvelope as prost::Message>::decode(encoded.as_slice()).expect("envelope decodes");

        let decoded: RemoteErrorEnvelope =
            serde_json::from_str(&request.payload_json).expect("envelope decodes");
        assert_eq!(decoded.error.message, "remote failed");
    }

    #[test]
    fn grpc_proto_contract_documents_json_envelope_lane() {
        let proto = include_str!("../../../contracts/grpc/lenso/remote/v1/remote_module.proto");

        assert!(proto.contains("service RemoteModule"));
        assert!(proto.contains("rpc GetManifest"));
        assert!(proto.contains("rpc ListAdminRecords"));
        assert!(proto.contains("rpc GetAdminRecord"));
        assert!(proto.contains("rpc InvokeAdminAction"));
        assert!(proto.contains("rpc QueryAdminValue"));
        assert!(proto.contains("rpc ProxyHttpRoute"));
        assert!(proto.contains("rpc InvokeFunction"));
        assert!(proto.contains("rpc HandleEvent"));
        assert!(proto.contains("message JsonEnvelope"));
        assert!(proto.contains("string payload_json = 1"));
    }

    #[tokio::test]
    async fn grpc_transport_loads_manifest_invokes_function_and_handles_event() {
        let base_url = spawn_remote_module_server().await;
        let config = RemoteModuleConfig::new("remote-grpc", base_url);

        let module = RemoteModuleSource::new(config.clone())
            .expect("source builds")
            .load()
            .await
            .expect("manifest loads over grpc");

        assert_eq!(config.transport, RemoteModuleTransport::Grpc);
        assert_eq!(module.manifest.name, "remote-grpc");
        assert_eq!(
            module
                .manifest
                .runtime
                .as_ref()
                .expect("runtime surface")
                .functions[0]
                .name,
            "remote_grpc.sync_contact.v1"
        );

        let admin_data = module.admin_data.as_ref().expect("grpc admin data source");
        let page = admin_data
            .list("contacts", &AdminListQuery::new(2, None))
            .await
            .expect("admin list invokes over grpc");
        assert_eq!(page.records.len(), 2);
        assert_eq!(page.next_cursor.as_deref(), Some("contact_2"));
        assert_eq!(page.records[0]["email"], "ada@example.com");

        let record = admin_data
            .get("contacts", "contact_1")
            .await
            .expect("admin get invokes over grpc")
            .expect("contact exists");
        assert_eq!(record["name"], "Ada Lovelace");

        let action_output = module
            .admin_actions
            .as_ref()
            .expect("grpc admin action source")
            .invoke("sync_contacts", json!({ "dry_run": true }))
            .await
            .expect("admin action invokes over grpc");
        assert_eq!(action_output["synced"], true);
        assert_eq!(action_output["dry_run"], true);

        let query_output = module
            .admin_queries
            .as_ref()
            .expect("grpc admin query source")
            .query("health")
            .await
            .expect("admin query invokes over grpc");
        assert_eq!(query_output["contacts"], 2);
        assert_eq!(query_output["healthy"], true);

        let proxy_output = proxy_http_route(
            &config,
            &RemoteHttpProxyInvokeRequest {
                request_id: "req_proxy_1".to_owned(),
                correlation_id: "corr_grpc_1".to_owned(),
                module_name: "remote-grpc".to_owned(),
                method: "GET".to_owned(),
                declared_path: "/contacts/{id}".to_owned(),
                remote_path: "/contacts/contact_1".to_owned(),
                path_params: [("id".to_owned(), "contact_1".to_owned())].into(),
                headers: [("accept".to_owned(), "application/json".to_owned())].into(),
                body: None,
            },
        )
        .await
        .expect("http proxy invokes over grpc");
        assert_eq!(proxy_output.status_code, 200);
        assert_eq!(proxy_output.body.expect("proxy body")["id"], "contact_1");

        let output = RemoteRuntimeFunction::new(config.clone(), "remote_grpc.sync_contact.v1")
            .expect("runtime function builds")
            .invoke(execution_context(), json!({ "contact_id": "contact_1" }))
            .await
            .expect("function invokes over grpc");
        assert_eq!(output["transport"], "grpc");
        assert_eq!(output["contact_id"], "contact_1");

        let event_response = handle_event(&config, &event_request())
            .await
            .expect("event handler invokes over grpc");
        assert_eq!(event_response.actions.len(), 1);
        match &event_response.actions[0] {
            RemoteEventResultAction::EnqueueFunction {
                function_name,
                input,
            } => {
                assert_eq!(function_name, "remote_grpc.sync_contact.v1");
                assert_eq!(input["contact_id"], "usr_1");
            }
        }
    }

    async fn spawn_remote_module_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test grpc server");
        let addr = listener.local_addr().expect("local addr");
        drop(listener);
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(TestRemoteModuleServer::default())
                .serve(addr)
                .await
                .expect("serve test grpc remote module");
        });
        sleep(TokioDuration::from_millis(25)).await;
        format!("grpc://{addr}")
    }

    fn execution_context() -> ExecutionContext {
        ExecutionContext {
            execution_id: ExecutionId("fnrun_grpc_1".to_owned()),
            function_name: "remote_grpc.sync_contact.v1".to_owned(),
            attempt: 1,
            queue: "remote-grpc".to_owned(),
            correlation_id: CorrelationId::new("corr_grpc_1"),
            causation_id: Some("test".to_owned()),
            actor: ActorContext::Service {
                service_id: "worker".to_owned(),
                scopes: vec!["runtime.functions.invoke".to_owned()],
            },
            tenant_id: None,
            trace: TraceContext::default(),
            deadline: None,
        }
    }

    fn event_request() -> RemoteEventHandleRequest {
        RemoteEventHandleRequest {
            request_id: "evt_1:sync_contact_on_user_registered".to_owned(),
            outbox_event_id: "evt_1".to_owned(),
            handler_name: "sync_contact_on_user_registered".to_owned(),
            event_name: "identity.user_registered.v1".to_owned(),
            event_version: 1,
            source_module: "identity".to_owned(),
            aggregate_type: "user".to_owned(),
            aggregate_id: "usr_1".to_owned(),
            correlation_id: "corr_grpc_1".to_owned(),
            causation_id: Some("httpreq_1".to_owned()),
            occurred_at: "2026-06-16T00:00:00Z".to_owned(),
            actor: ActorContext::Service {
                service_id: "worker".to_owned(),
                scopes: vec!["events.handle".to_owned()],
            },
            trace: TraceContext::default(),
            payload: json!({ "user_id": "usr_1" }),
            headers: json!({}),
        }
    }

    #[derive(Debug, Clone, Default)]
    struct TestRemoteModuleServer;

    impl<B> Service<http::Request<B>> for TestRemoteModuleServer
    where
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::Body>;
        type Error = Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(
            &mut self,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            match req.uri().path() {
                GET_MANIFEST_PATH
                | LIST_ADMIN_RECORDS_PATH
                | GET_ADMIN_RECORD_PATH
                | INVOKE_ADMIN_ACTION_PATH
                | QUERY_ADMIN_VALUE_PATH
                | PROXY_HTTP_ROUTE_PATH
                | INVOKE_FUNCTION_PATH
                | HANDLE_EVENT_PATH => {
                    struct JsonSvc {
                        path: &'static str,
                    }

                    impl UnaryService<JsonEnvelope> for JsonSvc {
                        type Response = JsonEnvelope;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

                        fn call(&mut self, request: tonic::Request<JsonEnvelope>) -> Self::Future {
                            let path = self.path;
                            Box::pin(async move {
                                grpc_json_response(path, request.into_inner())
                                    .map(tonic::Response::new)
                            })
                        }
                    }

                    let path = match req.uri().path() {
                        GET_MANIFEST_PATH => GET_MANIFEST_PATH,
                        LIST_ADMIN_RECORDS_PATH => LIST_ADMIN_RECORDS_PATH,
                        GET_ADMIN_RECORD_PATH => GET_ADMIN_RECORD_PATH,
                        INVOKE_ADMIN_ACTION_PATH => INVOKE_ADMIN_ACTION_PATH,
                        QUERY_ADMIN_VALUE_PATH => QUERY_ADMIN_VALUE_PATH,
                        PROXY_HTTP_ROUTE_PATH => PROXY_HTTP_ROUTE_PATH,
                        INVOKE_FUNCTION_PATH => INVOKE_FUNCTION_PATH,
                        HANDLE_EVENT_PATH => HANDLE_EVENT_PATH,
                        _ => unreachable!("matched paths above"),
                    };
                    Box::pin(async move {
                        let codec =
                            tonic_prost::ProstCodec::<JsonEnvelope, JsonEnvelope>::default();
                        let mut grpc = tonic::server::Grpc::new(codec);
                        Ok(grpc.unary(JsonSvc { path }, req).await)
                    })
                }
                _ => Box::pin(async move {
                    let mut response = http::Response::new(tonic::body::Body::default());
                    response.headers_mut().insert(
                        tonic::Status::GRPC_STATUS,
                        (tonic::Code::Unimplemented as i32).into(),
                    );
                    response.headers_mut().insert(
                        http::header::CONTENT_TYPE,
                        tonic::metadata::GRPC_CONTENT_TYPE,
                    );
                    Ok(response)
                }),
            }
        }
    }

    impl NamedService for TestRemoteModuleServer {
        const NAME: &'static str = "lenso.remote.v1.RemoteModule";
    }

    fn grpc_json_response(path: &str, request: JsonEnvelope) -> Result<JsonEnvelope, Status> {
        let payload = match path {
            GET_MANIFEST_PATH => serde_json::to_string(&manifest()).expect("manifest serializes"),
            LIST_ADMIN_RECORDS_PATH => {
                let request: RemoteAdminListRequest =
                    serde_json::from_str(&request.payload_json)
                        .map_err(|error| Status::invalid_argument(error.to_string()))?;
                if request.entity != "contacts" {
                    return Err(Status::not_found("unknown entity"));
                }
                serde_json::to_string(&RemoteListResponse {
                    records: contacts()
                        .into_iter()
                        .take(request.limit.clamp(1, 100) as usize)
                        .collect(),
                    next_cursor: Some("contact_2".to_owned()),
                })
                .expect("admin list response serializes")
            }
            GET_ADMIN_RECORD_PATH => {
                let request: RemoteAdminGetRequest = serde_json::from_str(&request.payload_json)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?;
                if request.entity != "contacts" || request.id != "contact_1" {
                    return Err(Status::not_found("unknown record"));
                }
                serde_json::to_string(&RemoteGetResponse {
                    record: Some(contact("contact_1", "Ada Lovelace", "ada@example.com")),
                })
                .expect("admin get response serializes")
            }
            INVOKE_ADMIN_ACTION_PATH => {
                let request: RemoteAdminActionInvokeRequest =
                    serde_json::from_str(&request.payload_json)
                        .map_err(|error| Status::invalid_argument(error.to_string()))?;
                if request.action != "sync_contacts" {
                    return Err(Status::not_found("unknown action"));
                }
                serde_json::to_string(&RemoteActionInvokeResponse {
                    result: json!({
                        "synced": true,
                        "dry_run": request.input["dry_run"].as_bool().unwrap_or(false),
                    }),
                })
                .expect("admin action response serializes")
            }
            QUERY_ADMIN_VALUE_PATH => {
                let request: RemoteAdminQueryRequest = serde_json::from_str(&request.payload_json)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?;
                if request.query != "health" {
                    return Err(Status::not_found("unknown query"));
                }
                serde_json::to_string(&RemoteQueryResponse {
                    data: json!({"contacts": 2, "healthy": true}),
                })
                .expect("admin query response serializes")
            }
            PROXY_HTTP_ROUTE_PATH => {
                let request: RemoteHttpProxyInvokeRequest =
                    serde_json::from_str(&request.payload_json)
                        .map_err(|error| Status::invalid_argument(error.to_string()))?;
                if request.method != "GET" || request.remote_path != "/contacts/contact_1" {
                    return Err(Status::not_found("unknown HTTP route"));
                }
                serde_json::to_string(&RemoteHttpProxyInvokeResponse {
                    status_code: 200,
                    body: Some(contact("contact_1", "Ada Lovelace", "ada@example.com")),
                })
                .expect("http proxy response serializes")
            }
            INVOKE_FUNCTION_PATH => {
                let request: RemoteFunctionInvokeRequest =
                    serde_json::from_str(&request.payload_json)
                        .map_err(|error| Status::invalid_argument(error.to_string()))?;
                if request.function_name != "remote_grpc.sync_contact.v1" {
                    return Err(Status::not_found("unknown function"));
                }
                let response = RemoteFunctionInvokeResponse {
                    output: json!({
                        "transport": "grpc",
                        "contact_id": request.input["contact_id"],
                    }),
                };
                serde_json::to_string(&response).expect("function response serializes")
            }
            HANDLE_EVENT_PATH => {
                let request: RemoteEventHandleRequest = serde_json::from_str(&request.payload_json)
                    .map_err(|error| Status::invalid_argument(error.to_string()))?;
                if request.handler_name != "sync_contact_on_user_registered" {
                    return Err(Status::not_found("unknown handler"));
                }
                serde_json::to_string(&RemoteEventHandleResponse {
                    actions: vec![RemoteEventResultAction::EnqueueFunction {
                        function_name: "remote_grpc.sync_contact.v1".to_owned(),
                        input: json!({ "contact_id": request.aggregate_id }),
                    }],
                })
                .expect("event response serializes")
            }
            _ => return Err(Status::unimplemented("unknown path")),
        };

        Ok(JsonEnvelope {
            payload_json: payload,
        })
    }

    fn manifest() -> ModuleManifest {
        ModuleManifest::builder("remote-grpc")
            .declarative_admin(AdminDeclarativeSurface {
                pages: vec![platform_module::AdminDeclarativePage {
                    name: "overview".to_owned(),
                    label: "Overview".to_owned(),
                    sections: vec![platform_module::AdminDeclarativeSection {
                        name: "health".to_owned(),
                        label: "Health".to_owned(),
                        component: platform_module::AdminDeclarativeComponent::QueryValue {
                            query: "health".to_owned(),
                            capability: "remote_grpc.health.read".to_owned(),
                            value_path: "contacts".to_owned(),
                        },
                    }],
                }],
                actions: vec![AdminAction {
                    name: "sync_contacts".to_owned(),
                    label: "Sync contacts".to_owned(),
                    capability: "remote_grpc.contacts.sync".to_owned(),
                    input_schema: None,
                    confirmation: None,
                    danger_level: AdminActionDangerLevel::Low,
                    operation: None,
                }],
                fallback_schema: Some(AdminSchema {
                    entities: vec![EntitySchema {
                        name: "contacts".to_owned(),
                        label: "Contacts".to_owned(),
                        read_capability: "remote_grpc.contacts.read".to_owned(),
                        fields: vec![FieldSchema {
                            name: "email".to_owned(),
                            label: "Email".to_owned(),
                            field_type: FieldType::String,
                            nullable: false,
                        }],
                    }],
                }),
            })
            .runtime(RuntimeSurface {
                functions: vec![RuntimeFunctionDeclaration {
                    name: "remote_grpc.sync_contact.v1".to_owned(),
                    version: 1,
                    queue: "remote-grpc".to_owned(),
                    input_schema: Some("remote_grpc.sync_contact.v1".to_owned()),
                    retry_policy: Some(RuntimeRetryPolicyDeclaration {
                        max_attempts: 3,
                        initial_delay_ms: 100,
                    }),
                    operation: None,
                }],
                schedules: vec![],
                workflows: vec![],
            })
            .events(EventSurface {
                handlers: vec![EventHandlerDeclaration {
                    name: "sync_contact_on_user_registered".to_owned(),
                    event_name: "identity.user_registered.v1".to_owned(),
                    operation: None,
                }],
            })
            .build()
    }

    fn contacts() -> Vec<serde_json::Value> {
        vec![
            contact("contact_1", "Ada Lovelace", "ada@example.com"),
            contact("contact_2", "Grace Hopper", "grace@example.com"),
        ]
    }

    fn contact(id: &str, name: &str, email: &str) -> serde_json::Value {
        json!({
            "id": id,
            "name": name,
            "email": email,
        })
    }
}
