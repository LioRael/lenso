use lenso_host::http::{
    ApiOpenApiRouter, DataResponse, Json, OpenApiRouter, json, routes,
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
struct AppStatusResponse {
    status: &'static str,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AppStatusResponseEnvelope)]
struct AppStatusResponseEnvelope {
    data: AppStatusResponse,
}

pub fn merge_http(base: ApiOpenApiRouter) -> ApiOpenApiRouter {
    base.merge(router())
}

fn router() -> ApiOpenApiRouter {
    OpenApiRouter::new().routes(routes!(status))
}

#[utoipa::path(
    get,
    path = "/v1/app/status",
    operation_id = "app_status",
    tag = "app",
    responses((
        status = 200,
        description = "App module status",
        body = AppStatusResponseEnvelope,
        content_type = "application/json"
    ))
)]
async fn status() -> Json<DataResponse<AppStatusResponse>> {
    json(AppStatusResponse { status: "ok" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_documents_status_route() {
        let document = router().to_openapi();

        assert!(document.paths.paths.contains_key("/v1/app/status"));
    }
}
