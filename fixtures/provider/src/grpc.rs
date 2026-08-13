use super::{ProviderFixtureState, descriptor};
use platform_provider::{
    ProviderHealth, ProviderInvocation, ProviderInvocationAcknowledgement, ProviderOperationKind,
};
use serde::Serialize;
use std::convert::Infallible;
use std::net::SocketAddr;
use tonic::codegen::{Body, BoxFuture, Service, StdError, http};
use tonic::server::{NamedService, UnaryService};
use tonic::{Request, Status};

const PREFIX: &str = "/lenso.provider.v1.Provider/";

#[derive(Clone, PartialEq, prost::Message)]
struct JsonEnvelope {
    #[prost(string, tag = "1")]
    payload_json: String,
}

pub async fn serve_grpc(address: SocketAddr) -> anyhow::Result<()> {
    tonic::transport::Server::builder()
        .add_service(ProviderGrpcServer {
            state: ProviderFixtureState::default(),
        })
        .serve(address)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
struct ProviderGrpcServer {
    state: ProviderFixtureState,
}

impl<B> Service<http::Request<B>> for ProviderGrpcServer
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
        let Some(method) = req.uri().path().strip_prefix(PREFIX) else {
            return unimplemented_response();
        };
        if operation_kind(method).is_none()
            && !matches!(
                method,
                "DescribeProvider" | "CheckHealth" | "GetInvocation" | "AcknowledgeInvocation"
            )
        {
            return unimplemented_response();
        }

        struct JsonSvc {
            method: String,
            state: ProviderFixtureState,
        }

        impl UnaryService<JsonEnvelope> for JsonSvc {
            type Response = JsonEnvelope;
            type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

            fn call(&mut self, request: Request<JsonEnvelope>) -> Self::Future {
                let method = self.method.clone();
                let state = self.state.clone();
                Box::pin(async move {
                    let payload =
                        handle(&state, &method, &request.into_inner().payload_json).await?;
                    Ok(tonic::Response::new(JsonEnvelope {
                        payload_json: payload,
                    }))
                })
            }
        }

        let method = method.to_owned();
        let state = self.state.clone();
        Box::pin(async move {
            let codec = tonic_prost::ProstCodec::<JsonEnvelope, JsonEnvelope>::default();
            let mut grpc = tonic::server::Grpc::new(codec);
            Ok(grpc.unary(JsonSvc { method, state }, req).await)
        })
    }
}

impl NamedService for ProviderGrpcServer {
    const NAME: &'static str = "lenso.provider.v1.Provider";
}

async fn handle(
    state: &ProviderFixtureState,
    method: &str,
    payload_json: &str,
) -> Result<String, Status> {
    match method {
        "DescribeProvider" => encode(&descriptor()),
        "CheckHealth" => {
            let value: ProviderHealth = serde_json::from_value(
                serde_json::to_value(super::health().await.0).map_err(internal)?,
            )
            .map_err(internal)?;
            encode(&value)
        }
        "GetInvocation" => {
            let reference: platform_provider::ProviderInvocationReference = decode(payload_json)?;
            let outcome = state
                .get(&reference.invocation_id)
                .await
                .ok_or_else(|| Status::not_found("invocation was not found"))?;
            encode(&outcome)
        }
        "AcknowledgeInvocation" => {
            let ack: ProviderInvocationAcknowledgement = decode(payload_json)?;
            let reference = state.acknowledge(&ack.invocation_id, &ack).await.map_err(
                |status| match status {
                    axum::http::StatusCode::NOT_FOUND => {
                        Status::not_found("invocation was not found")
                    }
                    _ => Status::failed_precondition("outcome digest did not match"),
                },
            )?;
            encode(&reference)
        }
        _ => {
            let invocation: ProviderInvocation = decode(payload_json)?;
            let outcome = state
                .invoke(
                    &invocation.export_key.clone(),
                    operation_kind(method).expect("validated method"),
                    invocation,
                )
                .await
                .map_err(|(_status, body)| Status::failed_precondition(body.0.to_string()))?;
            encode(&outcome)
        }
    }
}

fn operation_kind(method: &str) -> Option<ProviderOperationKind> {
    Some(match method {
        "InvokeHttpRoute" => ProviderOperationKind::HttpRoute,
        "ListAdminRecords" => ProviderOperationKind::AdminList,
        "GetAdminRecord" => ProviderOperationKind::AdminGet,
        "QueryAdminValue" => ProviderOperationKind::AdminQuery,
        "InvokeAdminAction" => ProviderOperationKind::AdminAction,
        "InvokeRuntimeFunction" => ProviderOperationKind::RuntimeFunction,
        "HandleEvent" => ProviderOperationKind::EventHandler,
        _ => return None,
    })
}

fn encode(value: &impl Serialize) -> Result<String, Status> {
    serde_json::to_string(value).map_err(internal)
}

fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, Status> {
    serde_json::from_str(value).map_err(|error| Status::invalid_argument(error.to_string()))
}

fn internal(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}

fn unimplemented_response() -> BoxFuture<http::Response<tonic::body::Body>, Infallible> {
    Box::pin(async move {
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
    })
}
