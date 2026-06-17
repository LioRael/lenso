use super::{
    CONTACTS, IDENTITY_USER_REGISTERED_EVENT, REMOTE_SYNC_CONTACT_FUNCTION,
    REMOTE_USER_REGISTERED_HANDLER, RuntimeFunctionInvokeRequest, contact_to_value,
    remote_crm_manifest,
};
use serde_json::{Value, json};
use std::convert::Infallible;
use std::net::SocketAddr;
use tonic::codegen::{Body, BoxFuture, Service, StdError, http};
use tonic::server::{NamedService, UnaryService};
use tonic::{Request, Status};

const GET_MANIFEST_PATH: &str = "/lenso.remote.v1.RemoteModule/GetManifest";
const LIST_ADMIN_RECORDS_PATH: &str = "/lenso.remote.v1.RemoteModule/ListAdminRecords";
const GET_ADMIN_RECORD_PATH: &str = "/lenso.remote.v1.RemoteModule/GetAdminRecord";
const INVOKE_ADMIN_ACTION_PATH: &str = "/lenso.remote.v1.RemoteModule/InvokeAdminAction";
const PROXY_HTTP_ROUTE_PATH: &str = "/lenso.remote.v1.RemoteModule/ProxyHttpRoute";
const INVOKE_FUNCTION_PATH: &str = "/lenso.remote.v1.RemoteModule/InvokeFunction";
const HANDLE_EVENT_PATH: &str = "/lenso.remote.v1.RemoteModule/HandleEvent";

#[derive(Clone, PartialEq, prost::Message)]
struct JsonEnvelope {
    // Keep the example on the first stable JSON envelope lane.
    #[prost(string, tag = "1")]
    payload_json: String,
}

pub async fn serve_grpc(address: SocketAddr) -> anyhow::Result<()> {
    tracing::info!(%address, "starting remote module fixture grpc server");
    tonic::transport::Server::builder()
        .add_service(RemoteModuleGrpcServer)
        .serve(address)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
struct RemoteModuleGrpcServer;

impl<B> Service<http::Request<B>> for RemoteModuleGrpcServer
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
            | PROXY_HTTP_ROUTE_PATH
            | INVOKE_FUNCTION_PATH
            | HANDLE_EVENT_PATH => {
                struct JsonSvc {
                    path: &'static str,
                }

                impl UnaryService<JsonEnvelope> for JsonSvc {
                    type Response = JsonEnvelope;
                    type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

                    fn call(&mut self, request: Request<JsonEnvelope>) -> Self::Future {
                        let path = self.path;
                        Box::pin(async move {
                            grpc_json_response(path, request.into_inner()).map(tonic::Response::new)
                        })
                    }
                }

                let path = match req.uri().path() {
                    GET_MANIFEST_PATH => GET_MANIFEST_PATH,
                    LIST_ADMIN_RECORDS_PATH => LIST_ADMIN_RECORDS_PATH,
                    GET_ADMIN_RECORD_PATH => GET_ADMIN_RECORD_PATH,
                    INVOKE_ADMIN_ACTION_PATH => INVOKE_ADMIN_ACTION_PATH,
                    PROXY_HTTP_ROUTE_PATH => PROXY_HTTP_ROUTE_PATH,
                    INVOKE_FUNCTION_PATH => INVOKE_FUNCTION_PATH,
                    HANDLE_EVENT_PATH => HANDLE_EVENT_PATH,
                    _ => unreachable!("matched paths above"),
                };
                Box::pin(async move {
                    let codec = tonic_prost::ProstCodec::<JsonEnvelope, JsonEnvelope>::default();
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

impl NamedService for RemoteModuleGrpcServer {
    const NAME: &'static str = "lenso.remote.v1.RemoteModule";
}

fn grpc_json_response(path: &str, request: JsonEnvelope) -> Result<JsonEnvelope, Status> {
    let payload = match path {
        GET_MANIFEST_PATH => serde_json::to_string(&remote_crm_manifest())
            .map_err(|error| Status::internal(error.to_string()))?,
        LIST_ADMIN_RECORDS_PATH => list_admin_records_payload(&request.payload_json)?,
        GET_ADMIN_RECORD_PATH => get_admin_record_payload(&request.payload_json)?,
        INVOKE_ADMIN_ACTION_PATH => invoke_admin_action_payload(&request.payload_json)?,
        PROXY_HTTP_ROUTE_PATH => proxy_http_route_payload(&request.payload_json)?,
        INVOKE_FUNCTION_PATH => invoke_function_payload(&request.payload_json)?,
        HANDLE_EVENT_PATH => handle_event_payload(&request.payload_json)?,
        _ => return Err(Status::unimplemented("unknown method")),
    };
    Ok(JsonEnvelope {
        payload_json: payload,
    })
}

fn invoke_function_payload(payload_json: &str) -> Result<String, Status> {
    let request: RuntimeFunctionInvokeRequest = serde_json::from_str(payload_json)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    if request.function_name != REMOTE_SYNC_CONTACT_FUNCTION {
        return Err(Status::not_found(format!(
            "runtime function {} was not found",
            request.function_name
        )));
    }

    serde_json::to_string(&json!({
        "output": {
            "synced": true,
            "contact_id": request
                .input
                .get("contact_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
            "request_id": request.request_id,
            "function_run_id": request.function_run_id,
            "attempt": request.attempt,
            "correlation_id": request.correlation_id,
            "causation_id": request.causation_id,
            "actor_kind": request
                .actor
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or(""),
            "trace_id": request
                .trace
                .get("trace_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
        }
    }))
    .map_err(|error| Status::internal(error.to_string()))
}

fn list_admin_records_payload(payload_json: &str) -> Result<String, Status> {
    let request: Value = serde_json::from_str(payload_json)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    if request.get("entity").and_then(Value::as_str) != Some("contacts") {
        return Err(Status::not_found("admin entity was not found"));
    }

    let cursor = request.get("cursor").and_then(Value::as_str);
    let start = cursor
        .and_then(|cursor| CONTACTS.iter().position(|contact| contact.id == cursor))
        .map_or(0, |index| index + 1);
    let limit = request
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(50)
        .clamp(1, 100) as usize;
    let records = CONTACTS
        .iter()
        .skip(start)
        .take(limit)
        .map(contact_to_value)
        .collect::<Vec<_>>();
    let next_cursor = (start + records.len() < CONTACTS.len())
        .then(|| records.last())
        .flatten()
        .and_then(|record| record.get("id"))
        .and_then(Value::as_str);

    serde_json::to_string(&json!({
        "records": records,
        "next_cursor": next_cursor,
    }))
    .map_err(|error| Status::internal(error.to_string()))
}

fn get_admin_record_payload(payload_json: &str) -> Result<String, Status> {
    let request: Value = serde_json::from_str(payload_json)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    if request.get("entity").and_then(Value::as_str) != Some("contacts") {
        return Err(Status::not_found("admin entity was not found"));
    }
    let id = request.get("id").and_then(Value::as_str).unwrap_or("");
    let Some(contact) = CONTACTS.iter().find(|contact| contact.id == id) else {
        return Err(Status::not_found("admin record was not found"));
    };

    serde_json::to_string(&json!({
        "record": contact_to_value(contact),
    }))
    .map_err(|error| Status::internal(error.to_string()))
}

fn invoke_admin_action_payload(payload_json: &str) -> Result<String, Status> {
    let request: Value = serde_json::from_str(payload_json)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    if request.get("action").and_then(Value::as_str) != Some("sync_contacts") {
        return Err(Status::not_found("admin action was not found"));
    }
    let input = request.get("input").unwrap_or(&Value::Null);

    serde_json::to_string(&json!({
        "result": {
            "synced": true,
            "dry_run": input
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            "contacts": CONTACTS.len(),
        }
    }))
    .map_err(|error| Status::internal(error.to_string()))
}

fn proxy_http_route_payload(payload_json: &str) -> Result<String, Status> {
    let request: Value = serde_json::from_str(payload_json)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    if request.get("remote_path").and_then(Value::as_str) == Some("/contacts")
        && request.get("method").and_then(Value::as_str) == Some("GET")
    {
        return serde_json::to_string(&json!({
            "status_code": 200,
            "body": {
                "contacts": CONTACTS.iter().map(contact_to_value).collect::<Vec<_>>(),
            }
        }))
        .map_err(|error| Status::internal(error.to_string()));
    }

    let path = request
        .get("remote_path")
        .and_then(Value::as_str)
        .unwrap_or("");
    let id = path.strip_prefix("/contacts/");
    let Some(contact) = id.and_then(|id| CONTACTS.iter().find(|contact| contact.id == id)) else {
        return serde_json::to_string(&json!({
            "status_code": 404,
            "body": {
                "error": {
                    "code": "not_found",
                    "message": format!("remote route {path} was not found"),
                    "retryable": false,
                    "details": []
                }
            }
        }))
        .map_err(|error| Status::internal(error.to_string()));
    };

    serde_json::to_string(&json!({
        "status_code": 200,
        "body": contact_to_value(contact),
    }))
    .map_err(|error| Status::internal(error.to_string()))
}

fn handle_event_payload(payload_json: &str) -> Result<String, Status> {
    let request: Value = serde_json::from_str(payload_json)
        .map_err(|error| Status::invalid_argument(error.to_string()))?;
    if request.get("handler_name").and_then(Value::as_str) != Some(REMOTE_USER_REGISTERED_HANDLER) {
        return Err(Status::not_found("event handler was not found"));
    }
    if request.get("event_name").and_then(Value::as_str) != Some(IDENTITY_USER_REGISTERED_EVENT) {
        return Err(Status::invalid_argument("unsupported event"));
    }

    let payload = request.get("payload").unwrap_or(&Value::Null);
    let contact_id = payload
        .get("user_id")
        .or_else(|| request.get("aggregate_id"))
        .and_then(Value::as_str)
        .unwrap_or("unknown_contact");
    let email = payload.get("email").and_then(Value::as_str).unwrap_or("");

    serde_json::to_string(&json!({
        "actions": [{
            "type": "enqueue_function",
            "function_name": REMOTE_SYNC_CONTACT_FUNCTION,
            "input": {
                "contact_id": contact_id,
                "email": email,
                "source_event_id": request
                    .get("outbox_event_id")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            }
        }]
    }))
    .map_err(|error| Status::internal(error.to_string()))
}
